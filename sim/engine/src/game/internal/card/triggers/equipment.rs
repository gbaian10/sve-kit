//! Completed attachments observe the new equipment through the shared pending queue.
use super::{
    Ability, AbilityKind, AbilityOrigin, Cause, EventPayload, EventSnapshotPolicy, Game, Result,
    SnapshotId, TriggerAggregation, TriggerCondition, ZoneKind, invalid, optional_payment,
    unsupported,
};
use crate::game::internal::Execution;
use crate::ir::events::equipment;
use crate::ir::{AbilityQualification, EquipEvent, LimitSpec};

fn observes(ability: &Ability) -> bool {
    matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && ability.trigger.as_ref().is_some_and(|trigger| {
            matches!(&trigger.on, TriggerCondition::Event { pattern }
                if equipment::binding(pattern).is_some())
        })
}

pub(in crate::game::internal) fn check_header(ability: &Ability) -> Result<bool> {
    if !observes(ability) {
        return Ok(false);
    }
    let trigger = ability
        .trigger
        .as_ref()
        .ok_or_else(|| invalid("missing equip header"))?;
    if ability.kind != AbilityKind::Triggered
        || ability.qualification != AbilityQualification::Ordinary
        || !ability.event_captures.is_empty()
        || !optional_payment::trigger_play(ability)
        || ability.active_zones != [ZoneKind::Equipment]
        || trigger.active_zones != [ZoneKind::Equipment]
        || trigger.view != EventSnapshotPolicy::AfterEntry
        || trigger.aggregation != (TriggerAggregation::PerFact {})
        || trigger
            .limit
            .is_some_and(|limit| !LimitSpec::is_triggered(limit))
    {
        return Err(unsupported("printed self equipment attachment header"));
    }
    Ok(true)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Equipment triggers use the existing event source and pending authority."
)]
impl Game {
    pub(in crate::game::internal) fn internal_equipped_event(
        &mut self,
        event: EquipEvent,
        execution: &Execution,
        before: SnapshotId,
    ) -> Result<()> {
        let holder = execution.object_id(event.equipment)?.to_owned();
        let abilities = self
            .internal_printed_abilities(&holder)?
            .into_iter()
            .filter(|ability| {
                ability.node.active_zones == [ZoneKind::Equipment] && observes(&ability.node)
            })
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
        let mut pending = Vec::new();
        for ability in abilities {
            pending.push(self.timing_occurrence(
                &holder,
                ability,
                &header,
                EventPayload::Equip { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }
}
