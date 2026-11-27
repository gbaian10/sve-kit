//! Effect results share the rules outcome and immediately release typed continuations.

use super::{Execution, Game, Result, seat_name, unsupported};
use crate::ir::{
    Ability, AbilityQualification, AbilitySource, Action, Instruction, Located, ObjectOperand,
    Outcome, PlayerRef, PlayerRelation, ZoneKind, victory,
};

pub(super) fn check_action(action: &Action, expected: &AbilitySource) -> Result<()> {
    if let Action::GameResult {
        players,
        result,
        source,
    } = action
        && (source != expected || !victory::effect_players(players, *result))
    {
        return Err(unsupported("effect result player or source"));
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

    pub(super) fn internal_game_result(
        &mut self,
        execution: &Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_action(&instruction.action, &instruction.source)?;
        let Action::GameResult {
            players, result, ..
        } = &instruction.action
        else {
            return Err(unsupported("effect result instruction"));
        };
        let seats = execution
            .players(self, players)?
            .into_iter()
            .map(|player| seat_name(player).map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        let frame = execution.frame();
        match result {
            Outcome::Win => self.win_for_players(seats, &frame),
            Outcome::Lose => self.lose_for_players(seats, &frame),
        }
    }
}
