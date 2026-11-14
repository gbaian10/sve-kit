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
                } | TurnOperation::SkipNext {},
                ..
            }
        )
    {
        return Err(unsupported("controller turn scheduling"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Typed extra turns mutate the shared authoritative schedule."
)]
impl Game {
    pub(super) fn internal_schedule_turn(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action)?;
        let player = execution.player(self, &PlayerRef::Controller {})?;
        if matches!(
            instruction.action,
            Action::ScheduleTurn {
                operation: TurnOperation::SkipNext {},
                ..
            }
        ) {
            self.skip_next_turn(seat_name(player)?.to_owned())?;
        } else {
            self.insert_extra_turns(seat_name(player)?.to_owned(), 1)?;
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::ScheduledTurn(ScheduleToken(self.state.ir_epoch)),
        );
        Ok(())
    }
}
