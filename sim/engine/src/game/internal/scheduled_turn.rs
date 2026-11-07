//! Typed insertion uses the same queue and next-turn selection as rule turns.

use super::{Execution, Game, InstructionOutcome, Result, seat_name, unsupported};
use crate::ir::{Action, Instruction, PlayerRef, ScheduleAnchor, ScheduleToken, TurnOperation};

pub(super) fn check_action(action: &Action) -> Result<()> {
    if matches!(action, Action::ScheduleTurn { .. })
        && !matches!(
            action,
            Action::ScheduleTurn {
                player: PlayerRef::Controller {},
                operation: TurnOperation::InsertBefore {
                    anchor: ScheduleAnchor::NextTurn {}
                },
                ..
            }
        )
    {
        return Err(unsupported("controller extra turn before next turn"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Typed extra turns mutate the shared authoritative schedule."
)]
impl Game {
    pub(super) fn internal_extra_turn(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let player = execution.player(self, &PlayerRef::Controller {})?;
        self.insert_extra_turns(seat_name(player)?.to_owned(), 1)?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::ScheduledTurn(ScheduleToken(self.state.ir_epoch)),
        );
        Ok(())
    }
}
