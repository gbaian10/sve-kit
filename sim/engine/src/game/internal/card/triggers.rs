//! Authority occurrences share one queue and return to state-based checks after each play.
use crate::ir::LimitSpec;
use crate::ir::events::timing::leave_filter;
use crate::ir::optional_payment;

use super::super::source::{SourceInformation, SourceSnapshot};
use super::super::{Answer, DecisionToken, DecisionView, InputDomain, player_id};
use super::{
    Ability, AbilityKind, AbilityOrigin, CardPlay, Context, Game, Located, PlayerId, Result,
    RuntimeAbilitySource, ZoneKind, invalid, printed_origin, unsupported,
};
use crate::game::{Frame, Object};
use crate::ir::{
    BatchId, Cause, DecisionId, EventHeader, EventId, EventPattern, EventPayload,
    EventSnapshotPolicy, ExecutionId, GameTime, KeywordId, LeaveFieldEvent, OriginFactId, Phase,
    RuleId, RuntimeVisibility, RuntimeZoneRef, SnapshotId, Timestamp, TriggerAggregation,
    TriggerCondition, TriggerSpec, TurnId,
};
use alloc::collections::BTreeMap;

mod entry;
pub(in crate::game::internal) mod entry_operand;
mod evolution;
mod quota;
mod timing;
pub(in crate::game::internal) use quota::Admission;
pub use quota::{QuotaDomain, QuotaSelection};

/// Only question-scoped handles and already public information reach a player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingOption {
    /// Handle valid only with the current question token.
    pub handle: u64,
    /// Catalogue identity disclosed by the occurrence, independent of its current location.
    pub card: String,
    /// Printed line coordinate identifying the public ability on this card.
    pub line: u64,
    /// Public event kind, with no physical source or snapshot identity.
    pub event: String,
}

#[derive(Clone, Debug)]
pub(super) struct PendingOccurrence {
    id: u64,
    holder: String,
    source: RuntimeAbilitySource,
    ability: Located<Ability>,
    information: SourceInformation,
    header: EventHeader,
    leave: Option<LeaveFieldEvent>,
    card: String,
    timing: Option<EventPayload>,
    field_position: u64,
}

pub(in crate::game) struct DeparturePlan {
    updates: Vec<(u64, SourceSnapshot)>,
    departures: Vec<(PendingOccurrence, Object)>,
}

pub(super) fn check_header(ability: &Ability) -> Result<()> {
    if matches!(
        ability.source.origin,
        AbilityOrigin::Keyword {
            keyword: KeywordId::Fanfare,
            ..
        }
    ) {
        return super::fanfare::check_header(ability);
    }
    if entry::check_header(ability)? || timing::check_header(ability)? {
        return Ok(());
    }
    let Some(trigger) = &ability.trigger else {
        return Err(unsupported("missing trigger"));
    };
    let TriggerCondition::Event {
        pattern: EventPattern::LeaveField { binding, .. },
    } = &trigger.on
    else {
        return Err(unsupported("trigger pattern"));
    };
    if !matches!(&ability.source.origin, AbilityOrigin::Keyword { keyword: KeywordId::Lastword, origin }
        if matches!(origin.as_ref(), AbilityOrigin::Printed { .. }))
        || ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::BeforeExit
        || trigger
            .limit
            .is_some_and(|limit| limit != LimitSpec::once_per_turn())
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger.on != TriggerSpec::lastword(*binding).on
    {
        return Err(unsupported("printed self LeaveField to Cemetery Lastword"));
    }
    Ok(())
}

const fn is_lastword(ability: &Ability) -> bool {
    matches!(
        ability.source.origin,
        AbilityOrigin::Keyword {
            keyword: KeywordId::Lastword,
            ..
        }
    )
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Pending order and departure producers share the authoritative game."
)]
impl Game {
    fn pending_header(
        &mut self,
        actor: PlayerId,
        cause: Cause,
        source: Option<RuntimeAbilitySource>,
        before: SnapshotId,
    ) -> Result<EventHeader> {
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let id = self.state.ir_epoch;
        Ok(EventHeader {
            id: EventId(id),
            origin: OriginFactId(id),
            batch: Some(BatchId(before.0)),
            execution: if let Cause::Effect { execution } = cause {
                Some(execution)
            } else {
                None
            },
            cause,
            actor,
            source,
            time: GameTime {
                turn: TurnId(self.state.schedule.current()?),
                phase: match self.state.turn["phase"].as_str() {
                    Some("end") => Phase::End,
                    Some("start") => Phase::Start,
                    _ => Phase::Main,
                },
                point: Timestamp(id),
            },
            before,
            after: SnapshotId(id),
            receipt: None,
            visibility: RuntimeVisibility::Public {},
        })
    }

    fn new_pending(
        &mut self,
        holder: &str,
        source: RuntimeAbilitySource,
        ability: Located<Ability>,
        information: SourceInformation,
        header: EventHeader,
        leave: Option<LeaveFieldEvent>,
    ) -> Result<PendingOccurrence> {
        self.card_runtime.next_pending = self
            .card_runtime
            .next_pending
            .checked_add(1)
            .ok_or_else(|| invalid("pending identities exhausted"))?;
        let card = self.object(holder)?.card.clone();
        Ok(PendingOccurrence {
            id: self.card_runtime.next_pending,
            holder: holder.into(),
            source,
            ability,
            information,
            header,
            leave,
            card,
            timing: None,
            field_position: u64::try_from(
                self.zone_ids(super::seat_name(source.controller)?, "field")
                    .iter()
                    .position(|id| id == holder)
                    .ok_or_else(|| invalid("trigger source has no public field position"))?,
            )
            .map_err(invalid)?
            .checked_add(1)
            .ok_or_else(|| invalid("field position overflow"))?,
        })
    }

    fn departure_cause(&mut self, id: &str, frame: &Frame) -> Result<Cause> {
        let cause = if let Some(number) = frame.cause["effect"]["execution"].as_u64() {
            Cause::Effect {
                execution: ExecutionId(number),
            }
        } else {
            let rule = frame
                .values
                .get("movement_rules")
                .and_then(|rules| rules.get(id))
                .and_then(serde_json::Value::as_str)
                .or_else(|| frame.cause["rule"].as_str())
                .ok_or_else(|| unsupported("departure without authority cause"))?;
            Cause::Rule {
                rule: self.rule_id(rule)?,
            }
        };
        Ok(cause)
    }

    /// Rule identities are allocated only by this authority map, never from caller numbers.
    fn rule_id(&mut self, rule: &str) -> Result<RuleId> {
        let next = RuleId(
            u64::try_from(self.card_runtime.rule_ids.len())
                .map_err(invalid)?
                .checked_add(1)
                .ok_or_else(|| invalid("rule identities exhausted"))?,
        );
        Ok(*self
            .card_runtime
            .rule_ids
            .entry(rule.into())
            .or_insert(next))
    }

    fn printed_departures(&self, object: &Object, to: &str) -> Result<Vec<Located<Ability>>> {
        let abilities = self.internal_printed_abilities(&object.id)?;
        if !self.card_runtime.cards.values().any(|program| {
            program.card().abilities.iter().any(|a| {
                matches!(
                    a.node.trigger.as_ref().map(|t| &t.on),
                    Some(TriggerCondition::Event {
                        pattern: EventPattern::LeaveField { .. }
                    })
                )
            })
        }) {
            return Ok(Vec::new());
        }
        let own_turn = object.controller == self.active();
        Ok(abilities
            .into_iter()
            .filter(|a| {
                if is_lastword(&a.node) {
                    return to == "cemetery";
                }
                let Some(TriggerSpec {
                    on:
                        TriggerCondition::Event {
                            pattern: EventPattern::LeaveField { binding, predicate },
                        },
                    ..
                }) = &a.node.trigger
                else {
                    return false;
                };
                leave_filter(*binding, predicate).is_some_and(|own| !own || own_turn)
            })
            .collect())
    }

    pub(in crate::game) fn internal_before_departures(
        &mut self,
        ids: &[String],
        destinations: &BTreeMap<String, String>,
        frame: &Frame,
    ) -> Result<DeparturePlan> {
        let mut plan = DeparturePlan {
            updates: Vec::new(),
            departures: Vec::new(),
        };
        let before = SnapshotId(self.state.ir_epoch);
        for pending in &self.card_runtime.pending {
            if ids.contains(&pending.holder)
                && destinations.get(&pending.holder).is_some_and(|to| {
                    self.object(&pending.holder).is_ok_and(|object| {
                        object.zone != *to
                            && object.generation == pending.source.holder.generation.0
                    })
                })
            {
                plan.updates.push((
                    pending.id,
                    pending.information.snapshot(self, pending.source)?,
                ));
            }
        }
        for id in ids {
            let object = self.object(id)?.clone();
            let Some(destination) = destinations.get(id).map(String::as_str) else {
                continue;
            };
            if object.zone != "field" || destination == "field" {
                continue;
            }
            let abilities = self.printed_departures(&object, destination)?;
            let Some(definition) = abilities.first().map(|ability| ability.node.id) else {
                continue;
            };
            // Only a subscribed departure needs a typed destination; other moves stay as before.
            let to = match destination {
                "cemetery" => ZoneKind::Cemetery,
                "hand" => ZoneKind::Hand,
                "deck" => ZoneKind::Deck,
                "banish" => ZoneKind::Banish,
                "ex" => ZoneKind::Ex,
                _ => return Err(unsupported("departure destination")),
            };
            let actor = player_id(&object.controller)?;
            let effect_source = self.card_runtime.active_source;
            let cause = self.departure_cause(id, frame)?;
            let holder = self.internal_card_source(id, definition, actor)?;
            let snapshot = self.information_snapshot(holder.holder)?.at(before);
            let header = self.pending_header(
                effect_source.map_or(player_id(self.active())?, |source| source.controller),
                cause,
                if matches!(cause, Cause::Rule { .. }) {
                    None
                } else {
                    effect_source
                },
                snapshot.view().snapshot,
            )?;
            for ability in abilities {
                let source = self.internal_card_source(id, ability.node.id, actor)?;
                let mut information = SourceInformation::current(self, source)?;
                information.seal(Some(snapshot.clone()));
                let leave = LeaveFieldEvent {
                    object_before: source.holder,
                    controller: actor,
                    lki: snapshot.view(),
                    to: RuntimeZoneRef {
                        player: player_id(&object.owner)?,
                        kind: to,
                    },
                };
                let occurrence = self.new_pending(
                    id,
                    source,
                    ability,
                    information,
                    header.clone(),
                    Some(leave),
                )?;
                plan.departures.push((occurrence, object.clone()));
            }
        }
        Ok(plan)
    }

    pub(in crate::game) fn internal_commit_departures(
        &mut self,
        plan: DeparturePlan,
    ) -> Result<()> {
        if plan.updates.is_empty() && plan.departures.is_empty() {
            return Ok(());
        }
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let after = SnapshotId(self.state.ir_epoch);
        for (id, snapshot) in plan.updates {
            if let Some(pending) = self
                .card_runtime
                .pending
                .iter_mut()
                .find(|pending| pending.id == id)
                && self
                    .state
                    .objects
                    .get(&pending.holder)
                    .is_some_and(|current| {
                        current.generation != snapshot.view().object.generation.0
                    })
            {
                pending.information.seal(Some(snapshot));
            }
        }
        let mut admitted = Vec::new();
        for (mut pending, previous) in plan.departures {
            if self.object(&previous.id)?.generation == previous.generation {
                return Err(invalid("departure did not commit"));
            }
            pending.header.after = after;
            admitted.push(pending);
        }
        self.internal_admit_pending(admitted)
    }

    /// Prepare one authority-owned pending-order question after all rule processing is stable.
    ///
    /// # Errors
    /// Rejects a busy execution or a required check timing without changing its input question.
    pub fn internal_pending_choice(&mut self) -> Result<Option<DecisionView>> {
        if !self.card_runtime.unadmitted.is_empty() {
            return Err(unsupported(
                "evolution resource printed abilities are not admitted",
            ));
        }
        if self.typed.is_some()
            || self.card_runtime.checkpoint.is_some()
            || !self.prepared.is_empty()
        {
            return Err(invalid("pending choice is not at a stable check timing"));
        }
        if let Some((question, _)) = &self.card_runtime.question {
            return Ok(Some(question.clone()));
        }
        if self.card_runtime.pending.is_empty() {
            return Ok(None);
        }
        let before = self.clone();
        let result = self.prepare_pending_order();
        if result.is_err() {
            let high = self.state.ir_epoch;
            *self = before;
            self.state.ir_epoch = self.state.ir_epoch.max(high);
        }
        result
    }

    fn prepare_pending_order(&mut self) -> Result<Option<DecisionView>> {
        if self.state.game["ended"] == true {
            self.card_runtime.pending.clear();
            return Ok(None);
        }
        let active = player_id(self.active())?;
        let actor = if self
            .card_runtime
            .pending
            .iter()
            .any(|pending| pending.source.controller == active)
        {
            active
        } else if let Some(pending) = self.card_runtime.pending.first() {
            pending.source.controller
        } else {
            return Ok(None);
        };
        let members = self
            .card_runtime
            .pending
            .iter()
            .filter(|pending| pending.source.controller == actor)
            .map(|pending| pending.id)
            .collect::<Vec<_>>();
        let options = self
            .card_runtime
            .pending
            .iter()
            .filter(|pending| pending.source.controller == actor)
            .enumerate()
            .map(|(index, pending)| {
                pending.public_option(
                    u64::try_from(index)
                        .map_err(invalid)?
                        .checked_add(1)
                        .ok_or_else(|| invalid("pending handles exhausted"))?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("IR epoch exhausted"))?;
        let question = DecisionView {
            actor,
            token: DecisionToken {
                execution: ExecutionId(self.state.ir_epoch),
                decision: DecisionId(0),
                epoch: self.state.ir_epoch,
            },
            domain: InputDomain::Pending(options),
        };
        self.card_runtime.question = Some((question.clone(), members));
        Ok(Some(question))
    }

    pub(super) fn answer_internal_pending(
        &mut self,
        actor: PlayerId,
        token: DecisionToken,
        answer: &Answer,
    ) -> Result<CardPlay> {
        let (question, members) = self
            .card_runtime
            .question
            .as_ref()
            .ok_or_else(|| invalid("no pending-order question"))?;
        if question.actor != actor || question.token != token {
            return Err(invalid("wrong actor or stale pending token"));
        }
        let Answer::Pending(handle) = answer else {
            return Err(invalid("wrong pending answer"));
        };
        let index = usize::try_from(
            handle
                .checked_sub(1)
                .ok_or_else(|| invalid("unknown pending handle"))?,
        )
        .map_err(invalid)?;
        let id = *members
            .get(index)
            .ok_or_else(|| invalid("unknown pending handle"))?;
        let position = self
            .card_runtime
            .pending
            .iter()
            .position(|pending| pending.id == id && pending.source.controller == actor)
            .ok_or_else(|| invalid("pending is no longer selectable"))?;
        let before = self.clone();
        let pending = self.card_runtime.pending.remove(position);
        self.card_runtime.question = None;
        let result = (|| {
            if self.state.frame.is_some()
                || self.state.prompt.is_some()
                || !self.state.pending.is_empty()
                || !self.state.continuous.is_empty()
                || !self.state.delayed.is_empty()
            {
                return Err(unsupported("unadmitted pending intervention"));
            }
            self.internal_prototype_abilities()?;
            self.check_internal_resources()?;
            if pending.header.after.0 < pending.header.before.0
                || pending.leave.is_some_and(|leave| {
                    leave.lki.snapshot != pending.header.before
                        || leave.object_before != pending.source.holder
                })
            {
                return Err(invalid("pending event and source proof disagree"));
            }
            pending.information.snapshot(self, pending.source)?;
            let context = Context {
                source: pending.holder,
                generation: pending.source.holder.generation,
                controller: pending.source.controller,
            };
            let mut execution = self.new_card_execution(context)?;
            if let (Some(EventPayload::EnterField { event }), Some(trigger)) =
                (&pending.timing, &pending.ability.node.trigger)
                && let TriggerCondition::Event {
                    pattern: EventPattern::EnterField { binding, .. },
                } = &trigger.on
            {
                execution.entry_object = Some((binding.key(), event.object));
            }
            if let (Some(EventPayload::Damage { event }), Some(trigger)) =
                (&pending.timing, &pending.ability.node.trigger)
                && let TriggerCondition::Event {
                    pattern: EventPattern::Damage { binding, .. },
                } = &trigger.on
            {
                execution.damage_event = Some((binding.key(), *event));
            }
            execution.ability = Some(pending.source);
            execution.trigger_occurrence = Some(pending.id);
            execution.source_information = Some(pending.information);
            self.run_card_play(execution, pending.ability.node)
        })();
        if matches!(result, Err(crate::EngineFailure::IllegalPlay(_))) {
            let high = self.state.ir_epoch;
            *self = before;
            self.state.ir_epoch = self.state.ir_epoch.max(high);
            self.card_runtime
                .pending
                .retain(|candidate| candidate.id != id);
            self.card_runtime.question = None;
            self.card_runtime.checkpoint = Some(token.execution);
            return Ok(CardPlay::Cancelled);
        }
        self.finish_card_attempt(before, result)
    }

    pub(super) fn internal_cancel_occurrence(&mut self, id: u64, checkpoint: ExecutionId) {
        self.typed = None;
        self.card_runtime
            .pending
            .retain(|candidate| candidate.id != id);
        self.card_runtime.question = None;
        self.card_runtime.checkpoint = Some(checkpoint);
    }
}

impl PendingOccurrence {
    fn public_option(&self, handle: u64) -> Result<PendingOption> {
        Ok(PendingOption {
            handle,
            card: self.card.clone(),
            line: printed_origin(&self.ability.node.source.origin)?.1.0,
            event: if let Some(event) = &self.timing {
                timing::public_event(event, self.field_position)?
            } else if self.leave.is_some() {
                format!("LeaveField from field slot {}", self.field_position)
            } else {
                // Every producer, including Fanfare, now stores its public event payload.
                return Err(invalid("pending occurrence without a public event"));
            },
        })
    }
}
