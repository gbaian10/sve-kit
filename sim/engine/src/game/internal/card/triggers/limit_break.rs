//! A field card observes its own completed limit break.

use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPayload, EventSnapshotPolicy, Game, Result,
    SnapshotId, TriggerAggregation, TriggerCondition, ZoneKind, optional_payment, unsupported,
};
use crate::game::internal::Execution;
use crate::ir::{EventPattern, LimitBreakEvent, LimitSpec};

fn observes(ability: &Ability) -> Option<bool> {
    let TriggerCondition::Event {
        pattern: EventPattern::LimitBreak { final_break },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    Some(final_break.0)
}

pub(super) fn check_header(ability: &Ability) -> Result<bool> {
    let Some(trigger) = ability
        .trigger
        .as_ref()
        .filter(|_| observes(ability).is_some())
    else {
        return Ok(false);
    };
    if ability.kind != AbilityKind::Triggered
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || !ability.event_captures.is_empty()
        || !optional_payment::trigger_play(ability)
        || ability.active_zones != [ZoneKind::Field]
        || trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
    {
        return Err(unsupported("printed self limit-break header"));
    }
    Ok(true)
}

impl Game {
    pub(in crate::game::internal) fn internal_limit_break_event(
        &mut self,
        holder: &str,
        event: LimitBreakEvent,
        execution: &Execution,
        before: SnapshotId,
    ) -> Result<()> {
        let abilities = self
            .internal_printed_abilities(holder)?
            .into_iter()
            .filter(|ability| observes(&ability.node) == Some(event.final_break))
            .collect::<Vec<_>>();
        if abilities.is_empty() {
            return Ok(());
        }
        let header = self.pending_header(
            execution.context.controller,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            before,
        )?;
        let pending = abilities
            .into_iter()
            .map(|ability| {
                self.timing_occurrence(holder, ability, &header, EventPayload::LimitBreak { event })
            })
            .collect::<Result<_>>()?;
        self.internal_admit_pending(pending)
    }
}
