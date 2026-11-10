//! Completed Drive facts capture field observers before the next check begins.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPattern, EventPayload, EventSnapshotPolicy,
    Game, Result, SnapshotId, TriggerAggregation, TriggerCondition, ZoneKind, invalid,
    optional_payment, unsupported,
};
use crate::game::internal::{Execution, seat_name};
use crate::ir::events::drive;
use crate::ir::{LimitSpec, PlayerId};

fn pattern(ability: &Ability) -> Option<&EventPattern> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return None;
    }
    let TriggerCondition::Event { pattern } = &ability.trigger.as_ref()?.on else {
        return None;
    };
    (drive::checked_binding(pattern).is_some() || drive::trigger_filter(pattern).is_some())
        .then_some(pattern)
}
pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if pattern(ability).is_none() {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("Drive event header"))?;
    if ability.kind != AbilityKind::Triggered
        || !optional_payment::trigger_play(ability)
        || ability.active_zones != [ZoneKind::Field]
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
    {
        return Err(unsupported("printed per-fact Drive field observer"));
    }
    Ok(true)
}
#[expect(
    clippy::multiple_inherent_impl,
    reason = "Drive observers share the authoritative pending queue."
)]
impl Game {
    pub(in crate::game::internal) fn internal_drive_event(
        &mut self,
        event: &EventPayload,
        player: PlayerId,
        execution: &Execution,
    ) -> Result<()> {
        let mut observers = Vec::new();
        let seat = seat_name(player)?;
        for holder in self.zone_ids(seat, "field") {
            let (card, _) = self.internal_effective_print(&holder)?;
            if !self
                .card_runtime
                .definitions
                .cards
                .get(&card)
                .is_some_and(|program| {
                    program
                        .card()
                        .abilities
                        .iter()
                        .any(|ability| pattern(&ability.node).is_some())
                })
            {
                continue;
            }
            for ability in self.internal_printed_abilities(&holder)? {
                if ability.node.active_zones != [ZoneKind::Field] {
                    continue;
                }
                let Some(pattern) = pattern(&ability.node) else {
                    continue;
                };
                let matches = match (event, pattern) {
                    (EventPayload::DriveChecked { .. }, EventPattern::DriveChecked { .. }) => true,
                    (EventPayload::DriveTrigger { .. }, EventPattern::DriveTrigger { .. }) => {
                        drive::trigger_filter(pattern)
                            .is_some_and(|own| !own || self.active() == seat)
                    }
                    _ => false,
                };
                if matches {
                    observers.push((holder.clone(), ability));
                }
            }
        }
        if observers.is_empty() {
            return Ok(());
        }
        let header = self.pending_header(
            player,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            SnapshotId(self.state.ir_epoch),
        )?;
        let mut pending = Vec::new();
        for (holder, ability) in observers {
            pending.push(self.timing_occurrence(&holder, ability, &header, event.clone())?);
        }
        self.internal_admit_pending(pending)
    }
}
