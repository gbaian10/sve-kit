//! Race facts enter the same per-fact pending queue as the other printed events.
use super::{Game, Result, player_id};
use crate::game::internal::Execution;
use crate::ir::{Cause, EventPattern, EventPayload, RaceEvent, SnapshotId, TriggerCondition};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Race facts share the authority's pending queue."
)]
impl Game {
    pub(in crate::game::internal) fn internal_race_event(
        &mut self,
        holder: &str,
        event: RaceEvent,
        execution: &Execution,
    ) -> Result<()> {
        let abilities = self
            .internal_printed_abilities(holder)?
            .into_iter()
            .filter(|ability| {
                matches!(
                    ability.node.trigger.as_ref().map(|trigger| &trigger.on),
                    Some(TriggerCondition::Event {
                        pattern: EventPattern::Race { .. }
                    })
                )
            })
            .collect::<Vec<_>>();
        if abilities.is_empty() {
            return Ok(());
        }
        let header = self.pending_header(
            player_id(&self.object(holder)?.controller)?,
            Cause::Effect {
                execution: execution.id,
            },
            execution.ability,
            SnapshotId(self.state.ir_epoch),
        )?;
        let mut pending = Vec::new();
        for ability in abilities {
            pending.push(self.timing_occurrence(
                holder,
                ability,
                &header,
                EventPayload::Race { event },
            )?);
        }
        self.internal_admit_pending(pending)
    }
}
