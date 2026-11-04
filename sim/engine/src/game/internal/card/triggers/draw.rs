//! Successful typed draws capture field observers before the instruction continues.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPattern, EventPayload, EventSnapshotPolicy,
    Game, Result, SnapshotId, TriggerAggregation, TriggerCondition, ZoneKind, invalid,
    optional_payment, unsupported,
};
use crate::game::internal::{Execution, seat_name};
use crate::ir::events::draw::predicate as draw_predicate;
use crate::ir::{DrawEvent, LimitSpec};

fn observes(ability: &Ability) -> bool {
    matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && matches!(ability.trigger.as_ref().map(|trigger| &trigger.on),
            Some(TriggerCondition::Event {
                pattern: EventPattern::Draw { binding, predicate }
            }) if *predicate == draw_predicate(*binding))
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    if !observes(ability) {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing draw header"))?;
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
        return Err(unsupported("printed own non-start per-card draw header"));
    }
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Draw observers share the authoritative pending queue."
)]
impl Game {
    fn draw_observers(&self, seat: &str) -> Result<Vec<(String, super::Located<Ability>)>> {
        let mut observers = Vec::new();
        if self.state.turn["phase"] == "start" {
            return Ok(observers);
        }
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

    pub(in crate::game::internal) fn internal_draw_event(
        &mut self,
        event: DrawEvent,
        execution: &Execution,
    ) -> Result<()> {
        let observers = self.draw_observers(seat_name(event.player)?)?;
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
                EventPayload::Draw { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }
}
