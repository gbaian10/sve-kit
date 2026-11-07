//! Effect victory shares the rules result and immediately releases typed continuations.

use super::{Execution, Game, Result, seat_name, unsupported};
use crate::ir::{
    AbilitySource, Action, Cardinality, Instruction, Nat, Outcome, PlayerRef, PlayerRelation,
    TextQuantifier,
};

pub(super) fn check_action(action: &Action, expected: &AbilitySource) -> Result<()> {
    if let Action::GameResult {
        players,
        result,
        source,
    } = action
        && (*result != Outcome::Win
            || source != expected
            || players.relative_to != (PlayerRef::Controller {})
            || players.relation != PlayerRelation::SelfPlayer
            || players.cardinality
                != (Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                })
            || players.text_quantifier != TextQuantifier::Implicit)
    {
        return Err(unsupported("controller-only effect victory"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Typed victory uses the common authoritative game result."
)]
impl Game {
    pub(super) fn internal_victory(
        &mut self,
        execution: &Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action, &instruction.source)?;
        let player = execution.player(self, &PlayerRef::Controller {})?;
        self.win_for_players(vec![seat_name(player)?.to_owned()], &execution.frame())
    }
}
