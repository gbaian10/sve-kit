use super::{Cause, EventPayload, Game, PendingOccurrence, Result, SnapshotId, invalid, player_id};
use crate::game::internal::Execution;
use crate::ir::{Nat, TargetedEvent, events::timing::targeted_filter};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Formal targets use the same pending occurrences as other timing events."
)]
impl Game {
    pub(in crate::game::internal) fn internal_targeted_occurrences(
        &mut self,
        execution: &Execution,
    ) -> Result<Vec<PendingOccurrence>> {
        let mut pending = Vec::new();
        if execution.target_declarations.is_empty() {
            return Ok(pending);
        }
        let active = player_id(self.active())?;
        let cause = Cause::Effect {
            execution: execution.id,
        };
        let before = SnapshotId(self.state.ir_epoch);
        for declaration in &execution.target_declarations {
            for (slot, object) in declaration.objects.iter().copied().enumerate() {
                let holder = execution.object_id(object)?;
                let current = self.object(holder)?;
                if current.zone != "field" || current.generation != object.generation.0 {
                    continue;
                }
                let controller = player_id(&current.controller)?;
                for ability in self.internal_effective_abilities(holder)? {
                    let Some(own_turn) = targeted_filter(&ability.node) else {
                        continue;
                    };
                    if own_turn && active != controller
                        || self.internal_trigger_forbidden(&ability.node, controller)?
                    {
                        continue;
                    }
                    let header =
                        self.pending_header(declaration.player, cause, execution.ability, before)?;
                    let event = EventPayload::Targeted {
                        event: TargetedEvent {
                            object,
                            by: execution.id,
                            selection: declaration.decision,
                            chooser: declaration.player,
                            target_slot: Nat(u64::try_from(slot).map_err(invalid)?),
                        },
                    };
                    pending.push(self.timing_occurrence(holder, ability, &header, event)?);
                }
            }
        }
        Ok(pending)
    }

    pub(in crate::game::internal) fn internal_commit_targeted(
        &mut self,
        pending: Vec<PendingOccurrence>,
    ) -> Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        self.internal_admit_pending(pending)
    }
}
