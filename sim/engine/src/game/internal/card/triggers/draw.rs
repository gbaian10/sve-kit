//! Successful draws capture field observers before the game continues.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventHeader, EventPattern, EventPayload,
    EventSnapshotPolicy, Game, Result, SnapshotId, TriggerAggregation, TriggerCondition, ZoneKind,
    invalid, optional_payment, player_id, unsupported,
};
use crate::game::internal::admission::PlannedAbility;
use crate::game::internal::{Execution, seat_name};
use crate::ir::events::draw::predicate as draw_predicate;
use crate::ir::{Bool, DrawEvent, Expr, LimitSpec, Nat};

#[derive(Clone, Copy, PartialEq, Eq)]
enum DrawScope {
    OwnNonStart,
    EveryPlayer,
}

fn scope(ability: &Ability) -> Option<DrawScope> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return None;
    }
    let TriggerCondition::Event {
        pattern: EventPattern::Draw { binding, predicate },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    if *predicate == Expr::Literal(Bool(true)) {
        Some(DrawScope::EveryPlayer)
    } else if *predicate == draw_predicate(*binding) {
        Some(DrawScope::OwnNonStart)
    } else {
        None
    }
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(scope) = scope(ability) else {
        return Ok(false);
    };
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
        || (scope == DrawScope::EveryPlayer && !ability.event_captures.is_empty())
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
    {
        return Err(unsupported("printed per-card draw header"));
    }
    Ok(true)
}

impl Game {
    fn draw_observers(&self, seat: &str) -> Result<Vec<(String, PlannedAbility)>> {
        let mut observers = Vec::new();
        for controller in ["P1", "P2"] {
            let includes_own = controller == seat && self.state.turn["phase"] != "start";
            let matches_draw = |ability: &Ability| {
                scope(ability).is_some_and(|scope| scope == DrawScope::EveryPlayer || includes_own)
            };
            for holder in self.zone_ids(controller, "field") {
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
                            .any(|ability| matches_draw(&ability.node))
                    })
                {
                    continue;
                }
                for ability in self.internal_printed_abilities(&holder)? {
                    if ability.node.active_zones == [ZoneKind::Field] && matches_draw(&ability.node)
                    {
                        observers.push((holder.clone(), ability));
                    }
                }
            }
        }
        Ok(observers)
    }

    pub(in crate::game) fn internal_rule_draw_event(
        &mut self,
        seat: &str,
        drawn: &str,
    ) -> Result<()> {
        let observers = self.draw_observers(seat)?;
        if observers.is_empty() {
            return Ok(());
        }
        let player = player_id(seat)?;
        let event = DrawEvent {
            player,
            object: self.internal_current_reference(drawn)?,
            ordinal: Nat(1),
            instruction: None,
        };
        let rule = self.rule_id("7.2.4")?;
        let header = self.pending_header(
            player,
            Cause::Rule { rule },
            None,
            SnapshotId(self.state.ir_epoch),
        )?;
        self.admit_draw_observers(observers, &header, event)
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
        self.admit_draw_observers(observers, &header, event)
    }

    fn admit_draw_observers(
        &mut self,
        observers: Vec<(String, PlannedAbility)>,
        header: &EventHeader,
        event: DrawEvent,
    ) -> Result<()> {
        let mut pending = Vec::new();
        for (holder, ability) in observers {
            pending.push(self.timing_occurrence(
                &holder,
                ability,
                header,
                EventPayload::Draw { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }
}
