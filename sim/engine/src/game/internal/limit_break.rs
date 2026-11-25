use super::{Execution, Game, InstructionOutcome, Result, unsupported};
use crate::game::limit_break::TREASURE;
use crate::ir::ObjectOperand;
use crate::ir::{Action, Instruction, LimitBreakEvent, SnapshotId};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Resolution retains the source reference that paid for the limit break."
)]
impl Game {
    pub(super) fn internal_limit_break(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::LimitBreak {
            holder,
            final_break,
        } = &instruction.action
        else {
            return Err(unsupported("limit-break instruction"));
        };
        let event = self.commit_limit_break(execution, holder, final_break.0)?;
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::LimitBreak(event));
        Ok(())
    }

    fn commit_limit_break(
        &mut self,
        execution: &Execution,
        operand: &ObjectOperand,
        final_break: bool,
    ) -> Result<Option<LimitBreakEvent>> {
        let source = &execution.context.source;
        let material = self.object(source)?;
        if self.object_type(source)? != TREASURE {
            return Ok(None);
        }
        let Some(holder) = execution.current_limit_break_subject(self, operand)? else {
            return Ok(None);
        };
        let id = execution.object_id(holder)?;
        if !self.can_limit_break(id, final_break)? {
            return Ok(None);
        }
        let reference = execution.reference(self, source)?;
        let available = match material.zone.as_str() {
            "ex" => material.generation == execution.context.generation.0,
            // The same effect can use its own material for both stages (CP05-054 Q&A).
            "limit_break" if final_break => execution.receipts.values().any(|outcome| {
                matches!(outcome, InstructionOutcome::LimitBreak(Some(event))
                    if event.holder == holder && event.material == reference && !event.final_break)
            }),
            _ => false,
        };
        if !available {
            return Ok(None);
        }
        let before = SnapshotId(self.state.ir_epoch);
        if !self.apply_limit_break(
            id,
            source,
            final_break,
            execution.timestamp,
            &execution.frame(),
        )? {
            return Ok(None);
        }
        let event = LimitBreakEvent {
            holder,
            material: execution.reference(self, source)?,
            final_break,
        };
        self.internal_limit_break_event(id, event, execution, before)?;
        Ok(Some(event))
    }
}
