//! Completed rule actions produce immutable contexts before Quick or end-phase progression.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventHeader, EventPattern, EventSnapshotPolicy,
    Game, KeywordId, Located, PendingOccurrence, Phase, Result, SnapshotId, SourceInformation,
    TriggerAggregation, TriggerCondition, TurnId, ZoneKind, invalid, player_id, unsupported,
};
use crate::game::Frame;
use crate::game::{Step, int, string};
use crate::ir::ExecutionId;
use crate::ir::LimitSpec;
use crate::ir::optional_payment;
use crate::ir::{
    AttackEvent, Edge, EventPayload as Event, Generation, Nat, ObjectRef, PhaseEvent, PlayerRef,
    RuntimeDamageRecipient,
};
use crate::ir::{DamageEvent, DamageFlags, PositiveInt, RuntimeObjectView};
use crate::ir::{EntryOrigin, Expr, ObjectFrozen, Predicate};
use core::num::NonZeroU64;
use serde_json::Value;

use crate::ir::events::timing::{
    DamageFilter, attack_predicate, evolve_predicate, opponent, phase_predicate, second_header,
};

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(trigger) = &ability.trigger else {
        return Ok(false);
    };
    let admitted = if super::evolution::observer_filter(ability).is_some() {
        true
    } else if let Some(valid) = second_header(ability) {
        valid
    } else {
        match (&ability.source.origin, &trigger.on) {
            (
                AbilityOrigin::Keyword {
                    keyword: KeywordId::OnRace,
                    origin,
                },
                TriggerCondition::Event {
                    pattern: EventPattern::Race { binding, predicate },
                },
            ) => {
                matches!(origin.as_ref(), AbilityOrigin::Printed { .. })
                    && *predicate
                        == Expr::Compute(Box::new(Predicate::ObjectEquals {
                            left: Expr::Frozen(Box::new(ObjectFrozen::RaceSubject {
                                event: *binding,
                            })),
                            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                        }))
            }
            (
                AbilityOrigin::Keyword {
                    keyword: KeywordId::OnAttack,
                    origin,
                },
                TriggerCondition::Event {
                    pattern: EventPattern::Attack { binding, predicate },
                },
            ) => {
                matches!(origin.as_ref(), AbilityOrigin::Printed { .. })
                    && *predicate == attack_predicate(*binding)
            }
            (
                AbilityOrigin::Keyword {
                    keyword: KeywordId::OnEvolve,
                    origin,
                },
                TriggerCondition::Event {
                    pattern: EventPattern::Evolve { binding, predicate },
                },
            ) => {
                matches!(origin.as_ref(), AbilityOrigin::Printed { .. })
                    && *predicate == evolve_predicate(*binding)
            }
            _ => return Ok(false),
        }
    };
    if !admitted
        || ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || (trigger.active_zones != [ZoneKind::Field] && !super::ex_phase(ability))
        || trigger.view
            != if matches!(
                trigger.on,
                TriggerCondition::Event {
                    pattern: EventPattern::LeaveField { .. }
                }
            ) {
                EventSnapshotPolicy::BeforeExit
            } else {
                EventSnapshotPolicy::AfterEntry
            }
        || trigger
            .limit
            .is_some_and(|limit| limit != LimitSpec::once_per_turn())
        || trigger.aggregation != (TriggerAggregation::PerFact {})
    {
        return Err(unsupported("printed per-fact timing or self-event header"));
    }
    Ok(true)
}

pub(super) fn public_event(event: &Event, position: u64) -> Result<String> {
    match event {
        Event::Race { event } => Ok(format!("Race {} at field slot {position}", event.ordinal.0)),
        Event::EnterField { event } => {
            let origin = match event.from {
                EntryOrigin::Zone { zone } => format!("from {zone:?}"),
                EntryOrigin::Other {} => "by creation".into(),
            };
            Ok(format!(
                "EnterField {:?} {origin} at field slot {position}",
                event.characters.names
            ))
        }
        Event::EnterEx { .. } => Ok(format!("EnterEx while source at field slot {position}")),
        Event::Destroy { .. } => Ok(format!("Destroy while source at field slot {position}")),
        Event::Play { .. } => Ok(format!("Play while source at cemetery slot {position}")),
        Event::Attack { event } => Ok(format!(
            "Attack {} from field slot {position}",
            event.ordinal.0
        )),
        Event::Phase { event } => Ok(format!(
            "{:?} Begin turn {} player {}",
            event.phase, event.turn.0, event.player.seat
        )),
        Event::Evolve { event } => Ok(format!(
            "Evolve {:?} from field slot {position}",
            event.kind
        )),
        Event::Damage { event } => Ok(format!(
            "Damage {} at field slot {position}",
            event.amount.0.get()
        )),
        _ => Err(invalid("unadmitted pending rule event")),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule actions and event producers share the authoritative game."
)]
impl Game {
    /// Apply an admitted rule action with loaded internal cards, stopping before pending abilities.
    ///
    /// # Errors
    /// Rejects a busy authority, prototype card plays, or an unsupported intervention atomically.
    pub fn internal_rule_action(&mut self, decision: &Value) -> Result<Step> {
        if !matches!(
            string(&decision["do"]),
            "evolve" | "attack" | "end-phase" | "guard-act" | "end-discard" | "mulligan"
        ) || self.typed.is_some()
            || self.card_runtime.checkpoint.is_some()
            || self.card_runtime.has_pending()
            || self.card_runtime.question.is_some()
            || self.card_runtime.quick.is_some()
            || !self.prepared.is_empty()
        {
            return Err(unsupported("internal rule action or timing"));
        }
        self.internal_prototype_abilities()?;
        self.check_internal_resources()?;
        let node = format!("internal-rule-{}", self.state.next_decision);
        let result = self.apply_rule_decision(decision, &node)?;
        self.state.next_decision = self.state.next_decision.saturating_add(1);
        Ok(result)
    }

    fn timing_abilities(&self, holder: &str) -> Result<Vec<Located<Ability>>> {
        Ok(self
            .internal_printed_abilities(holder)?
            .into_iter()
            .filter(|a| is_timing(&a.node))
            .collect())
    }

    pub(super) fn timing_occurrence(
        &mut self,
        holder: &str,
        ability: Located<Ability>,
        header: &EventHeader,
        event: Event,
    ) -> Result<PendingOccurrence> {
        let actor = player_id(&self.object(holder)?.controller)?;
        let source = self.internal_card_source(holder, ability.node.id, actor)?;
        let information = SourceInformation::current(self, source)?;
        let mut pending =
            self.new_pending(holder, source, ability, information, header.clone(), None)?;
        pending.timing = Some(event);
        Ok(pending)
    }

    pub(in crate::game) fn internal_attack_event(
        &mut self,
        holder: &str,
        target: &str,
        before: SnapshotId,
    ) -> Result<()> {
        let abilities = self
            .timing_abilities(holder)?
            .into_iter()
            .filter(|a| {
                matches!(
                    a.node.trigger.as_ref().map(|t| &t.on),
                    Some(TriggerCondition::Event {
                        pattern: EventPattern::Attack { .. }
                    })
                )
            })
            .collect::<Vec<_>>();
        if abilities.is_empty() {
            return Ok(());
        }
        let actor = player_id(&self.object(holder)?.controller)?;
        let source = self.internal_card_source(
            holder,
            abilities
                .first()
                .ok_or_else(|| invalid("missing attacker ability"))?
                .node
                .id,
            actor,
        )?;
        let target = if let Some(seat) = target.strip_suffix(".leader") {
            RuntimeDamageRecipient::Leader {
                player: player_id(seat)?,
            }
        } else {
            let generation = self.object(target)?.generation;
            self.internal_object_reference(target, generation)?
        };
        let event = AttackEvent {
            attacker: source.holder,
            target,
            controller: actor,
            ordinal: Nat(
                u64::try_from(int(&self.object(holder)?.state["attacks_this_turn"]))
                    .map_err(invalid)?,
            ),
        };
        let header =
            self.pending_header(actor, Cause::PlayerAction { player: actor }, None, before)?;
        let mut pending = Vec::new();
        for ability in abilities {
            pending.push(self.timing_occurrence(
                holder,
                ability,
                &header,
                Event::Attack { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Actual hits and their authority cause commit at one damage boundary."
    )]
    pub(in crate::game) fn internal_damage_events(
        &mut self,
        hits: &[Value],
        facts: Option<&[DamageEvent]>,
        frame: &Frame,
        before: SnapshotId,
    ) -> Result<()> {
        if !self.card_runtime.definitions.cards.values().any(|program| {
            program.card().abilities.iter().any(|a| {
                matches!(
                    a.node.trigger.as_ref().map(|t| &t.on),
                    Some(TriggerCondition::Event {
                        pattern: EventPattern::Damage { .. }
                    })
                )
            })
        }) {
            return Ok(());
        }
        let mut pending = Vec::new();
        for (index, hit) in hits.iter().enumerate() {
            let holder = string(&hit["target"]);
            if holder.ends_with(".leader") {
                continue;
            }
            let controller = player_id(&self.object(holder)?.controller)?;
            let event = if let Some(facts) = facts {
                *facts
                    .get(index)
                    .ok_or_else(|| invalid("missing actual damage fact"))?
            } else {
                if hit["battle"] != true {
                    return Err(unsupported("unadmitted damage producer"));
                }
                let source = string(&hit["source"]);
                DamageEvent {
                    source: RuntimeObjectView {
                        object: self.internal_current_reference(source)?,
                        snapshot: before,
                    },
                    recipient: RuntimeDamageRecipient::Object {
                        object: self.internal_current_reference(holder)?,
                    },
                    amount: PositiveInt(
                        NonZeroU64::new(u64::try_from(int(&hit["amount"])).map_err(invalid)?)
                            .ok_or_else(|| invalid("nonpositive actual damage"))?,
                    ),
                    flags: if hit["attack"] == true {
                        DamageFlags::AttackCombat
                    } else {
                        DamageFlags::Combat
                    },
                    controller: player_id(&self.object(source)?.controller)?,
                }
            };
            let active = player_id(self.active())?;
            let abilities = self
                .timing_abilities(holder)?
                .into_iter()
                .filter(|ability| {
                    let Some(trigger) = &ability.node.trigger else {
                        return false;
                    };
                    let TriggerCondition::Event {
                        pattern: EventPattern::Damage { binding, predicate },
                    } = &trigger.on
                    else {
                        return false;
                    };
                    DamageFilter::parse(*binding, predicate).is_some_and(|filter| {
                        (!filter.ability || event.flags == DamageFlags::Ability)
                            && filter
                                .own_turn
                                .is_none_or(|own| own == (controller == active))
                    })
                })
                .collect::<Vec<_>>();
            if abilities.is_empty() {
                continue;
            }
            if event.recipient
                != (RuntimeDamageRecipient::Object {
                    object: self.internal_current_reference(holder)?,
                })
            {
                return Err(invalid("damage recipient proof mismatch"));
            }
            let (cause, actor, source) = if facts.is_some() {
                let execution = frame.cause["effect"]["execution"]
                    .as_u64()
                    .ok_or_else(|| invalid("missing damage execution cause"))?;
                let source = self.card_runtime.active_source;
                (
                    Cause::Effect {
                        execution: ExecutionId(execution),
                    },
                    source.map_or(event.controller, |s| s.controller),
                    source,
                )
            } else {
                (
                    Cause::Rule {
                        rule: self.rule_id("8.4.9")?,
                    },
                    active,
                    None,
                )
            };
            let header = self.pending_header(actor, cause, source, before)?;
            for ability in abilities {
                pending.push(self.timing_occurrence(
                    holder,
                    ability,
                    &header,
                    Event::Damage { event },
                )?);
            }
        }
        self.internal_admit_pending(pending)
    }

    fn internal_object_reference(
        &mut self,
        holder: &str,
        generation: u64,
    ) -> Result<RuntimeDamageRecipient> {
        let id = self
            .internal_identities()?
            .into_iter()
            .find(|(_, physical)| physical == holder)
            .map(|(id, _)| id)
            .ok_or_else(|| invalid("missing rule object identity"))?;
        Ok(RuntimeDamageRecipient::Object {
            object: ObjectRef {
                id,
                generation: Generation(generation),
            },
        })
    }

    pub(in crate::game) fn internal_phase_event(
        &mut self,
        phase: Phase,
        before: SnapshotId,
    ) -> Result<()> {
        let actor = player_id(self.active())?;
        let mut candidates = Vec::new();
        for seat in ["P1", "P2"] {
            for holder in self
                .zone_ids(seat, "field")
                .into_iter()
                .chain(self.zone_ids(seat, "ex"))
            {
                for ability in self.timing_abilities(&holder)? {
                    let Some(trigger) = &ability.node.trigger else {
                        continue;
                    };
                    let zone = if self.object(&holder)?.zone == "ex" {
                        ZoneKind::Ex
                    } else {
                        ZoneKind::Field
                    };
                    if !trigger.active_zones.contains(&zone) {
                        continue;
                    }
                    if let TriggerCondition::Event {
                        pattern: EventPattern::Phase { binding, predicate },
                    } = &trigger.on
                    {
                        let controller = player_id(&self.object(&holder)?.controller)?;
                        let player = if controller == actor {
                            PlayerRef::Controller {}
                        } else {
                            opponent()
                        };
                        if *predicate == phase_predicate(*binding, phase, None)
                            || *predicate == phase_predicate(*binding, phase, Some(player))
                        {
                            candidates.push((holder.clone(), ability));
                        }
                    }
                }
            }
        }
        if candidates.is_empty() {
            return Ok(());
        }
        let event = PhaseEvent {
            turn: TurnId(self.state.schedule.current()?),
            player: actor,
            phase,
            edge: Edge::Begin,
        };
        let rule = self.rule_id(match phase {
            Phase::Main => "7.3.1",
            Phase::End => "7.4.1",
            Phase::Start => return Err(unsupported("start phase event")),
        })?;
        let header = self.pending_header(actor, Cause::Rule { rule }, None, before)?;
        let mut pending = Vec::new();
        for (holder, ability) in candidates {
            pending.push(self.timing_occurrence(
                &holder,
                ability,
                &header,
                Event::Phase { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }
}

pub(super) fn is_timing(ability: &Ability) -> bool {
    matches!(
        ability.trigger.as_ref().map(|t| &t.on),
        Some(TriggerCondition::Event {
            pattern: EventPattern::Attack { .. }
                | EventPattern::Phase { .. }
                | EventPattern::Evolve { .. }
                | EventPattern::Damage { .. }
        })
    )
}
