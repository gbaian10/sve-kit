//! Accepted die rolls capture field observers before the instruction continues.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPattern, EventPayload, EventSnapshotPolicy,
    Game, Located, PendingOccurrence, Result, SnapshotId, TriggerAggregation, TriggerCondition,
    ZoneKind, invalid, optional_payment, unsupported,
};
use crate::game::internal::{Execution, seat_name};
use crate::ir::dice::predicate as dice_predicate;
use crate::ir::{DiceEvent, LimitSpec};

fn observes(ability: &Ability) -> bool {
    matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && matches!(ability.trigger.as_ref().map(|trigger| &trigger.on),
            Some(TriggerCondition::Event {
                pattern: EventPattern::Dice { binding, predicate }
            }) if *predicate == dice_predicate(*binding))
}

/// A resolving observer reads the accepted roll through its own header binding.
pub(super) fn bind_event(execution: &mut Execution, pending: &PendingOccurrence) {
    if let (Some(EventPayload::Dice { event }), Some(trigger)) =
        (&pending.timing, &pending.ability.node.trigger)
        && let TriggerCondition::Event {
            pattern: EventPattern::Dice { binding, .. },
        } = &trigger.on
    {
        execution.dice_event = Some((*binding, event.clone()));
    }
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if !observes(ability) {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing dice header"))?;
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
        return Err(unsupported("printed own accepted die header"));
    }
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Dice observers share the authoritative pending queue."
)]
impl Game {
    fn dice_observers(&self, seat: &str) -> Result<Vec<(String, Located<Ability>)>> {
        let mut observers = Vec::new();
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
                        .any(|ability| observes(&ability.node))
                })
            {
                continue;
            }
            for ability in self.internal_printed_abilities(&holder)? {
                if ability.node.active_zones == [ZoneKind::Field] && observes(&ability.node) {
                    observers.push((holder.clone(), ability));
                }
            }
        }
        Ok(observers)
    }

    pub(in crate::game::internal) fn internal_dice_event(
        &mut self,
        event: &DiceEvent,
        execution: &Execution,
    ) -> Result<()> {
        let observers = self.dice_observers(seat_name(event.roller)?)?;
        if observers.is_empty() {
            return Ok(());
        }
        let header = self.pending_header(
            execution.context.controller,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            SnapshotId(self.state.ir_epoch),
        )?;
        let mut pending = Vec::new();
        for (holder, ability) in observers {
            pending.push(self.timing_occurrence(
                &holder,
                ability,
                &header,
                EventPayload::Dice {
                    event: event.clone(),
                },
            )?);
        }
        self.internal_admit_pending(pending)
    }
}
