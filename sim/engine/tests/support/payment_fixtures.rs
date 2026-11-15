//! Shared payment positions use the same C0 input in lib unit and integration tests.

use super::board_fixtures;
use serde_json::{Value, json};
use sve_engine::game::Game;
use sve_engine::ir::WellFormedCardProgram;
use sve_engine::{EngineFailure, Result};

pub(crate) fn x_document(expanded: &Value, head: &str, body: Value) -> Result<Value> {
    let prototype = expanded
        .pointer("/cards/BP01-041")
        .ok_or_else(fixture_error)?;
    let meta = prototype.get("meta").ok_or_else(fixture_error)?.clone();
    let mut entry = prototype
        .pointer("/abilities/0")
        .ok_or_else(fixture_error)?
        .clone();
    let fields = entry.as_object_mut().ok_or_else(fixture_error)?;
    fields.remove("activate");
    fields.insert(head.into(), body);
    let mut tree = expanded.clone();
    *tree.get_mut("cards").ok_or_else(fixture_error)? =
        json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    Ok(tree)
}

pub(crate) fn optional_document(expanded: &Value) -> Result<Value> {
    let prototype = expanded
        .pointer("/cards/BP08-038")
        .ok_or_else(fixture_error)?;
    let meta = prototype.get("meta").ok_or_else(fixture_error)?.clone();
    let mut entry = prototype
        .pointer("/abilities/0")
        .ok_or_else(fixture_error)?
        .clone();
    entry.as_object_mut().ok_or_else(fixture_error)?.insert("fanfare".into(), json!({
        "optional_cost":{"pp":2_i32,"result":"paid"},
        "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1_i32}],
        "do":[{"if":{"cond":{"paid":{"receipt":"paid"}},"then":[{"damage":{"to":"enemy","n":1_i32}}]}}]
    }));
    let mut tree = expanded.clone();
    *tree.get_mut("cards").ok_or_else(fixture_error)? =
        json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    Ok(tree)
}

pub(crate) fn x_board(program: &WellFormedCardProgram, pp: u8) -> Result<Game> {
    let mut initial = board_fixtures::setup(1, json!([]));
    *initial
        .pointer_mut("/players/P1/pp")
        .ok_or_else(fixture_error)? = json!({"current":pp,"max":pp});
    let zones = initial
        .pointer_mut("/players/P1/zones")
        .and_then(Value::as_object_mut)
        .ok_or_else(fixture_error)?;
    zones.insert("hand".into(), json!([]));
    zones.insert("field".into(), json!([{"id":"s","card":"unit-follower"}]));
    let mut game = Game::new(
        board_fixtures::catalog()?,
        &initial,
        &Value::Null,
        &Value::Null,
        "X-PP",
    )?;
    game.load_internal_card("unit-follower", program)?;
    Ok(game)
}

fn fixture_error() -> EngineFailure {
    EngineFailure::Invalid("incomplete C0 payment fixture".into())
}
