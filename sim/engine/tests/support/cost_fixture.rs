//! Cost positions use the shared synthetic board facts.

use super::board_fixtures;
use serde_json::{Value, json};
use sve_engine::game::Game;
use sve_engine::game::player::Viewer;
use sve_engine::ir::{CardProgram, WellFormedCardProgram};
use sve_engine::{EngineFailure, Result};

pub(crate) fn board(viewer: Viewer, secret: &str) -> Result<Game> {
    let mut initial = board_fixtures::setup(viewer.player().seat, json!([]));
    let (actor, opponent) = if viewer == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    *initial
        .pointer_mut("/turn/active")
        .ok_or_else(|| fixture_error("turn"))? = json!(actor);
    let own = initial
        .pointer_mut(&format!("/players/{actor}/zones"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| fixture_error("actor zones"))?;
    own.insert(
        "hand".into(),
        json!([
            {"id":"quoted-a","card":"unit-follower"}, {"id":"quoted-b","card":"unit-follower"}
        ]),
    );
    own.insert(
        "ex".into(),
        json!([{"id":"quoted-ex","card":"unit-follower"}]),
    );
    own.insert(
        "field".into(),
        json!([{"id":"quote-source","card":"unit-follower"}]),
    );
    let other = initial
        .pointer_mut(&format!("/players/{opponent}/zones"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| fixture_error("opponent zones"))?;
    other.insert(
        "hand".into(),
        json!([{"id":"opponent-secret","card":secret}]),
    );
    let mut game = Game::new(
        board_fixtures::catalog()?,
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-costs",
    )?;
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .map_err(|error| EngineFailure::Invalid(error.to_string()))?;
    for number in ["unit-follower", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &empty)?;
    }
    Ok(game)
}

fn fixture_error(field: &str) -> EngineFailure {
    EngineFailure::Invalid(format!("missing cost fixture {field}"))
}
