use super::{Execution, Game, InstructionOutcome, Result, seat_name, unsupported};
use crate::game::int;
use crate::ir::player_rules::PlayerRule;
use crate::ir::{
    AbilitySource, Action, InstallationSubjects, InstallationTime, Instruction, PlayerId, PlayerRef,
};

pub(super) fn check(action: &Action, expected: &AbilitySource) -> Result<PlayerRule> {
    let Action::InstallEffect {
        subjects: InstallationSubjects::Players { players, .. },
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return Err(unsupported("player rule installation"));
    };
    PlayerRule::from_effect(effect)
        .filter(|rule| rule.source(players, effect, duration) == Some(expected))
        .ok_or_else(|| unsupported("player rule or source"))
}

impl Game {
    pub(super) fn internal_install_player_rule(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let rule = check(&instruction.action, &instruction.source)?;
        let seat = seat_name(execution.player(self, &PlayerRef::Controller {})?)?;
        match rule {
            PlayerRule::UnlimitedHand => self.player_mut(seat)?.unlimited_hand = true,
            PlayerRule::CannotLoseDefense => self.player_mut(seat)?.defense_protected = true,
            PlayerRule::DoubleFanfare => {
                let next_own_turn = int(&self.state.turn["elapsed_turns"][seat]).saturating_add(1);
                self.player_mut(seat)?.double_fanfare.insert(next_own_turn);
            }
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Installed(vec![]));
        Ok(())
    }

    pub(in crate::game::internal) fn internal_fanfare_doubled(
        &self,
        player: PlayerId,
    ) -> Result<bool> {
        let seat = seat_name(player)?;
        Ok(seat == self.active()
            && self
                .player(seat)?
                .double_fanfare
                .contains(&int(&self.state.turn["elapsed_turns"][seat])))
    }

    pub(in crate::game) fn internal_expire_fanfare_multiplier(&mut self) -> Result<()> {
        let seat = self.active().to_owned();
        let turn = int(&self.state.turn["elapsed_turns"][&seat]);
        self.player_mut(&seat)?
            .double_fanfare
            .retain(|ordinal| *ordinal > turn);
        Ok(())
    }
}
