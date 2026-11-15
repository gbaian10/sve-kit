//! Current public turn facts retain the authority's seat order and unknowns.

use super::ProjectionError;
use crate::game::player::{Viewer, observation::TurnObservation};
use crate::ir::Phase;
use sve_battle_contract::{Nat, Seat, observation as wire};

pub(crate) fn project_turn(turn: &TurnObservation) -> Result<wire::TurnView, ProjectionError> {
    let [first, second] = turn.elapsed_turns;
    Ok(wire::TurnView {
        active: turn.active.map(seat),
        first_player: turn.first_player.map(seat),
        phase: turn.phase.map(|phase| match phase {
            Phase::Start => wire::Phase::Start,
            Phase::Main => wire::Phase::Main,
            Phase::End => wire::Phase::End,
        }),
        elapsed_turns: [Nat::try_from(first)?, Nat::try_from(second)?],
    })
}

const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}
