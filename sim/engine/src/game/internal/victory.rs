//! Effect victory shares the rules result and immediately releases typed continuations.

use super::{Execution, Game, Result, seat_name, unsupported};
use crate::ir::{
    Ability, AbilityQualification, AbilitySource, Action, Cardinality, Instruction, Located, Nat,
    ObjectOperand, Outcome, PlayerRef, PlayerRelation, TextQuantifier, ZoneKind, victory,
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

pub(super) fn check_static(located: &Located<Ability>) -> Result<()> {
    let ability = &located.node;
    if !victory::is_static(ability)
        || ability.active_zones != [ZoneKind::Field]
        || ability.qualification != AbilityQualification::Ordinary
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return Err(unsupported("static result permission header"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Typed victory uses the common authoritative game result."
)]
impl Game {
    pub(in crate::game) fn internal_result_forbidden(&self, seat: &str, win: bool) -> Result<bool> {
        for provider in self.field_ids() {
            let (card, _) = self.internal_effective_print(&provider)?;
            if !self.card_runtime.is_loaded(&card) {
                continue;
            }
            let own = self.object(&provider)?.controller == seat;
            for located in self.internal_printed_abilities(&provider)? {
                if !victory::is_static(&located.node) {
                    continue;
                }
                for effect in &located.node.declarations {
                    if let Some((relation, forbids_win)) = victory::permission(effect)
                        && win == forbids_win
                        && own == (*relation == PlayerRelation::SelfPlayer)
                    {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

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
