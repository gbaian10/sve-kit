//! Completed rule actions produce immutable contexts before Quick or end-phase progression.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventHeader, EventPattern, EventSnapshotPolicy,
    Game, KeywordId, Located, Phase, Result, SnapshotId, SourceInformation, TriggerAggregation,
    TriggerCondition, TurnId, ZoneKind, invalid, player_id, unsupported,
};
use crate::game::{Step, int, string};
use crate::ir::EntryOrigin;
use crate::ir::LimitSpec;
use crate::ir::{
    AttackEvent, Edge, EventPayload as Event, Generation, Nat, ObjectRef, PhaseEvent, PlayerRef,
    RuntimeDamageRecipient,
};
use serde_json::Value;

use crate::ir::events::timing::{attack_predicate, end_predicate, evolve_predicate, opponent};

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(trigger) = &ability.trigger else {
        return Ok(false);
    };
    let admitted = match (&ability.source.origin, &trigger.on) {
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
        (
            AbilityOrigin::Printed { .. },
            TriggerCondition::Event {
                pattern: EventPattern::Phase { binding, predicate },
            },
        ) => [None, Some(PlayerRef::Controller {}), Some(opponent())]
            .into_iter()
            .any(|player| *predicate == end_predicate(*binding, player)),
        _ => return Ok(false),
    };
    if !admitted
        || ability.kind != AbilityKind::Triggered
        || ability.play.is_some()
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger
            .limit
            .is_some_and(|limit| limit != LimitSpec::once_per_turn())
        || trigger.aggregation != (TriggerAggregation::PerFact {})
    {
        return Err(unsupported("printed self Attack or End Begin header"));
    }
    Ok(true)
}

pub(super) fn public_event(event: &Event, position: u64) -> Result<String> {
    match event {
        Event::EnterField { event } => {
            let EntryOrigin::Zone { zone } = event.from else {
                return Err(invalid("missing public entry origin"));
            };
            Ok(format!(
                "EnterField {:?} from {zone:?} at field slot {position}",
                event.characters.names
            ))
        }
        Event::Attack { event } => Ok(format!(
            "Attack {} from field slot {position}",
            event.ordinal.0
        )),
        Event::Phase { event } => Ok(format!(
            "End Begin turn {} player {}",
            event.turn.0, event.player.seat
        )),
        Event::Evolve { event } => Ok(format!(
            "Evolve {:?} from field slot {position}",
            event.kind
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
            // Guard followers carry a prototype keyword, which loaded internal cards reject.
            "evolve" | "attack" | "end-phase" | "pass" | "end-discard"
        ) || self.typed.is_some()
            || self.card_runtime.checkpoint.is_some()
            || self.card_runtime.has_pending()
            || self.card_runtime.question.is_some()
            || !self.prepared.is_empty()
        {
            return Err(unsupported("internal rule action or timing"));
        }
        self.internal_prototype_abilities()?;
        self.check_internal_cards()?;
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

    pub(super) fn timing_pending(
        &mut self,
        holder: &str,
        ability: Located<Ability>,
        header: &EventHeader,
        event: Event,
    ) -> Result<()> {
        let actor = player_id(&self.object(holder)?.controller)?;
        let source = self.internal_card_source(holder, ability.node.id, actor)?;
        let information = SourceInformation::current(self, source)?;
        let mut pending =
            self.new_pending(holder, source, ability, information, header.clone(), None)?;
        pending.timing = Some(event);
        self.internal_admit_pending(vec![pending])
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
        for ability in abilities {
            self.timing_pending(holder, ability, &header, Event::Attack { event })?;
        }
        Ok(())
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

    pub(in crate::game) fn internal_end_event(&mut self, before: SnapshotId) -> Result<()> {
        let actor = player_id(self.active())?;
        let mut candidates = Vec::new();
        for seat in ["P1", "P2"] {
            for holder in self.zone_ids(seat, "field") {
                for ability in self.timing_abilities(&holder)? {
                    let Some(trigger) = &ability.node.trigger else {
                        continue;
                    };
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
                        if *predicate == end_predicate(*binding, None)
                            || *predicate == end_predicate(*binding, Some(player))
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
            phase: Phase::End,
            edge: Edge::Begin,
        };
        let rule = self.rule_id("7.4.1")?;
        let header = self.pending_header(actor, Cause::Rule { rule }, None, before)?;
        for (holder, ability) in candidates {
            self.timing_pending(&holder, ability, &header, Event::Phase { event })?;
        }
        Ok(())
    }
}

pub(super) fn is_timing(ability: &Ability) -> bool {
    matches!(
        ability.trigger.as_ref().map(|t| &t.on),
        Some(TriggerCondition::Event {
            pattern: EventPattern::Attack { .. }
                | EventPattern::Phase { .. }
                | EventPattern::Evolve { .. }
        })
    )
}
