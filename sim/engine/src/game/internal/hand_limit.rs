use super::{Execution, Game, InstructionOutcome, Result, seat_name, unsupported};
use crate::ir::{
    AbilitySource, Action, InstallationSubjects, InstallationTime, Instruction, PlayerRef,
    player_hand_limit,
};

pub(super) fn check(action: &Action, expected: &AbilitySource) -> Result<()> {
    let Action::InstallEffect {
        subjects: InstallationSubjects::Players { players, .. },
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return Err(unsupported("unlimited hand installation"));
    };
    if player_hand_limit::source(players, effect, duration) == Some(expected) {
        return Ok(());
    }
    Err(unsupported("unlimited hand rule or source"))
}

impl Game {
    pub(super) fn internal_install_unlimited_hand(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check(&instruction.action, &instruction.source)?;
        let seat = seat_name(execution.player(self, &PlayerRef::Controller {})?)?;
        self.player_mut(seat)?.unlimited_hand = true;
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Installed(vec![]));
        Ok(())
    }
}
