//! An explicit hand reveal observes only the revealed object's own printed abilities.

use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPattern, EventPayload, EventSnapshotPolicy,
    Game, Result, SnapshotId, TriggerAggregation, TriggerCondition, ZoneKind, invalid,
    optional_payment, unsupported,
};
use crate::game::internal::Execution;
use crate::ir::ObjectRef;

fn observes(ability: &Ability) -> bool {
    matches!(
        ability.trigger.as_ref().map(|trigger| &trigger.on),
        Some(TriggerCondition::Event {
            pattern: EventPattern::SelfHandReveal {},
        })
    )
}

pub(in crate::game::internal) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(trigger) = ability.trigger.as_ref().filter(|_| observes(ability)) else {
        return Ok(false);
    };
    if ability.kind != AbilityKind::Triggered
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || !ability.event_captures.is_empty()
        || !optional_payment::trigger_play(ability)
        || ability.active_zones != [ZoneKind::Hand]
        || trigger.active_zones != [ZoneKind::Hand]
        || trigger.view != EventSnapshotPolicy::AtReveal
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger.limit.is_some()
    {
        return Err(unsupported("printed self hand-reveal header"));
    }
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Successful hand disclosures use the authoritative pending queue."
)]
impl Game {
    pub(in crate::game::internal) fn internal_hand_reveal_events(
        &mut self,
        members: &[(String, ObjectRef)],
        execution: &Execution,
        before: SnapshotId,
    ) -> Result<()> {
        let mut pending = Vec::new();
        let player = execution.context.controller;
        for (holder, card) in members {
            let (printing, _) = self.internal_effective_print(holder)?;
            if !self
                .card_runtime
                .definitions
                .cards
                .get(&printing)
                .is_some_and(|program| {
                    program
                        .card()
                        .abilities
                        .iter()
                        .any(|ability| observes(&ability.node))
                })
            {
                continue;
            }
            let header = self.pending_header(
                player,
                Cause::Effect {
                    execution: execution.id,
                },
                execution.ability,
                before,
            )?;
            for ability in self.internal_printed_abilities(holder)? {
                if !observes(&ability.node) || ability.node.active_zones != [ZoneKind::Hand] {
                    continue;
                }
                let occurrence = self.timing_occurrence(
                    holder,
                    ability,
                    &header,
                    EventPayload::HandReveal {
                        card: *card,
                        player,
                    },
                )?;
                if occurrence.source.holder != *card {
                    return Err(invalid("hand reveal source generation"));
                }
                pending.push(occurrence);
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        self.internal_admit_pending(pending)
    }
}
