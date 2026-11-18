use super::{Execution, Game, InstructionOutcome, Result, seat_name, unsupported};
use crate::game::int;
use crate::ir::{
    Instruction,
    phase_restrictions::{self, Restriction},
};

#[derive(Clone, Debug)]
pub(super) struct Installed {
    seat: String,
    turn: i64,
    phase: &'static str,
    action: &'static str,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Player restrictions follow the authoritative turn and phase state."
)]
impl Game {
    pub(super) fn internal_install_phase_restriction(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let (restriction, player) = phase_restrictions::projection(&instruction.action)
            .ok_or_else(|| unsupported("phase restriction"))?;
        let seat = seat_name(execution.player(self, player)?)?;
        let (action, phase) = match restriction {
            Restriction::Draw => ("normal_draw", "start"),
            Restriction::MaximumPp => ("normal_max_pp_gain", "start"),
            Restriction::FollowerPlay => ("play_follower", "main"),
        };
        let elapsed = int(&self.state.turn["elapsed_turns"][seat]);
        let current_turn =
            seat == self.active() && phase == "main" && self.state.turn["phase"] == "start";
        self.card_runtime.phase_restrictions.push(Installed {
            seat: seat.to_owned(),
            turn: elapsed.saturating_add(i64::from(!current_turn)),
            phase,
            action,
        });
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Installed(vec![]));
        Ok(())
    }

    pub(in crate::game) fn internal_phase_restricted(&self, seat: &str, action: &str) -> bool {
        seat == self.active()
            && self.card_runtime.phase_restrictions.iter().any(|effect| {
                effect.seat == seat
                    && effect.action == action
                    && self.state.turn["phase"] == effect.phase
                    && int(&self.state.turn["elapsed_turns"][seat]) == effect.turn
            })
    }

    pub(in crate::game) fn internal_phase_restrictions_end_turn(&mut self) {
        let seat = self.active().to_owned();
        let elapsed = int(&self.state.turn["elapsed_turns"][&seat]);
        self.card_runtime
            .phase_restrictions
            .retain(|effect| effect.seat != seat || effect.turn > elapsed);
    }
}
