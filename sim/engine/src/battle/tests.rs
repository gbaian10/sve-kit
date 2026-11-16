//! Direct component inputs stay inside the authority crate.

use crate::ir::CardKind;
use board_fixtures::{exact_authority, object};
use cost_fixture::board;
use serde_json::{Value, json};

#[path = "../../tests/support/board_fixtures.rs"]
pub(super) mod board_fixtures;
#[path = "../../tests/support/cost_fixture.rs"]
mod cost_fixture;

mod costs;
mod history;

#[path = "../../tests/support/board_fixture.rs"]
pub(super) mod board_fixture;
#[path = "../../tests/support/rule_fixtures.rs"]
mod rule_fixtures;

mod effects;
mod pending;

mod cost_flow;
#[path = "../../tests/support/payment_fixtures.rs"]
pub(super) mod payment_fixtures;

#[path = "../../tests/support/history_fixture.rs"]
mod history_fixture;
mod history_flow;
