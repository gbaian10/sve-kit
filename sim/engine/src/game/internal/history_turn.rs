//! Read the existing committed-play counter; only Combo projects a pending formal card.
use super::{Execution, Fact, Game, Nat, Result, invalid, seat_name, unsupported};
use crate::ir::events::timing::opponent;
use crate::ir::history_turn::{PlayedBy, played_by};
use crate::ir::{ExecutionRef, HistoryQuery, PlayerRef, Requirement, Requirements};

pub(super) fn public_requirement(requirements: &Requirements, requirement: &Requirement) -> bool {
    let suffix = match requirement.capability.code {
        "read.event-field" | "shape.PlayerFrozen.PlayController" => {
            "/query/pattern/predicate/compute/left/frozen"
        }
        "predicate.player-equals" | "shape.Predicate.PlayerEquals" => {
            "/query/pattern/predicate/compute"
        }
        _ => return false,
    };
    requirement.locations.iter().all(|location| {
        location.ir_path.strip_suffix(suffix).is_some_and(|read| {
            requirements.required.iter().any(|other| {
                other.capability.code == "read.history.count"
                    && other.locations.iter().any(|at| at.ir_path == read)
            })
        })
    })
}

fn count(execution: &Execution, game: &Game, player: &PlayerRef, pending: bool) -> Result<Nat> {
    let player = execution.player(game, player)?;
    let count = game
        .state
        .counters
        .get(&format!("{}.cards_played", seat_name(player)?))
        .copied()
        .unwrap_or_default();
    let mut count = u64::try_from(count).map_err(invalid)?;
    // CR 13.2 includes the card whose formal play is being checked, before it commits.
    if pending
        && player == execution.context.controller
        && (execution.spell || execution.field_entry)
        && !execution.facts.iter().any(|fact| {
            matches!(fact,
            Fact::CardPlayed { execution: id, .. } if *id == execution.id)
        })
    {
        count = count
            .checked_add(1)
            .ok_or_else(|| invalid("Combo overflow"))?;
    }
    Ok(Nat(count))
}

pub(super) fn combo(
    execution: &Execution,
    game: &Game,
    player: &PlayerRef,
    reference: ExecutionRef,
) -> Result<Nat> {
    if reference != (ExecutionRef::Current {}) {
        return Err(unsupported("Combo execution"));
    }
    count(execution, game, player, true)
}

pub(super) fn played(execution: &Execution, game: &Game, query: &HistoryQuery) -> Result<Nat> {
    if let PlayedBy::Player(player) = played_by(query)
        .ok_or_else(|| unsupported("unfiltered current-turn played-card history"))?
    {
        count(execution, game, &player, false)
    } else {
        let own = count(execution, game, &PlayerRef::Controller {}, false)?.0;
        let other = count(execution, game, &opponent(), false)?.0;
        own.checked_add(other)
            .map(Nat)
            .ok_or_else(|| invalid("played-card count overflow"))
    }
}
