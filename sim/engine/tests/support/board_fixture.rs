//! Shared by integration tests and lib unit tests via `#[path]`.
//! Only public engine APIs are used so both compilation contexts keep the same facts.

use super::{board_fixtures, rule_fixtures};
use alloc::sync::Arc;
use core::fmt::Display;
use core::result::Result as BuildResult;
use serde_json::{Value, json};
use std::sync::OnceLock;
use sve_battle_contract::{CardRef, Nat};
use sve_engine::battle::{BattleReferences, PrintingFaceIdentity, ReferenceInput};
use sve_engine::catalog::Catalog;
use sve_engine::game::Game;
use sve_engine::game::player::Viewer;
use sve_engine::ir::{CardProgram, WellFormedCardProgram};
use sve_engine::{EngineFailure, Result};

pub(crate) struct Fixture {
    pub(crate) catalog: Arc<Catalog>,
    pub(crate) references: BattleReferences,
    pub(crate) snapshot: String,
}

pub(crate) fn catalog_for(snapshot: &str) -> Result<Catalog> {
    let (_, keywords, document) = board_fixtures::catalog_documents(&json!("1"));
    let rules = rule_fixtures::rules(
        snapshot,
        json!([rule_fixtures::title(
            "synthetic",
            "unit-follower",
            json!([])
        )]),
        json!([]),
    );
    rule_fixtures::catalog(
        snapshot,
        &keywords,
        &[("synthetic".into(), document)],
        &rules.to_string(),
    )
}

pub(crate) fn fixture() -> Result<&'static Fixture> {
    static FIXTURE: OnceLock<BuildResult<Fixture, String>> = OnceLock::new();
    FIXTURE
        .get_or_init(|| build().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| EngineFailure::Invalid(error.clone()))
}

fn build() -> Result<Fixture> {
    let (base_snapshot, _, _) = board_fixtures::catalog_documents(&json!("1"));
    let mut rows = Vec::new();
    for line in base_snapshot.lines() {
        let mut row: Value = serde_json::from_str(line).map_err(fixture_error)?;
        if row.get("number").and_then(Value::as_str) == Some("unit-follower") {
            let faces = row
                .get_mut("faces")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| fixture_error("missing faces"))?;
            let first = faces
                .first_mut()
                .ok_or_else(|| fixture_error("missing face"))?;
            first
                .as_object_mut()
                .ok_or_else(|| fixture_error("face type"))?
                .insert("title".into(), json!("synthetic-title"));
            let second = first.clone();
            faces.push(second);
        }
        rows.push(row.to_string());
    }
    let snapshot = rows.join("\n");
    let catalog = Arc::new(catalog_for(&snapshot)?);
    let faces = [
        ("unit-follower", 0_u64, "printing-follower", 1_u32),
        ("unit-follower", 1, "printing-follower", 0),
        ("unit-spell", 0, "printing-spell", 0),
        ("unit-amulet", 0, "printing-amulet", 0),
    ]
    .into_iter()
    .map(
        |(number, native_face, printing, face)| PrintingFaceIdentity {
            card_no: number.into(),
            native_face,
            card: CardRef {
                printing_id: printing.into(),
                face: Nat::from(face),
            },
        },
    )
    .collect();
    let references = BattleReferences::new(
        &catalog,
        ReferenceInput {
            rules: None,
            wording: None,
            region: "jp".into(),
            snapshot_sha256: rule_fixtures::hash(snapshot.as_bytes()),
            faces,
        },
    )
    .map_err(fixture_error)?;
    Ok(Fixture {
        catalog,
        references,
        snapshot,
    })
}

pub(crate) fn empty_board(actor: Viewer, secret: &str) -> Result<Game> {
    board(actor, secret, &empty_program()?)
}

pub(crate) fn board(actor: Viewer, secret: &str, follower: &WellFormedCardProgram) -> Result<Game> {
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    let (own, opponent) = if actor == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    *initial
        .pointer_mut("/turn/active")
        .ok_or_else(|| fixture_error("missing turn"))? = json!(own);
    let own_zones = initial
        .pointer_mut(&format!("/players/{own}/zones"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| fixture_error("missing own zones"))?;
    own_zones.insert("hand".into(), json!([]));
    own_zones.insert("field".into(), json!([{"id":"s","card":"unit-follower"}]));
    let other_zones = initial
        .pointer_mut(&format!("/players/{opponent}/zones"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| fixture_error("missing opponent zones"))?;
    other_zones.insert("hand".into(), json!([{"id":"secret","card":secret}]));
    let mut game = Game::new(
        Arc::clone(&fixture()?.catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-pending",
    )?;
    game.load_internal_card("unit-follower", follower)?;
    let empty = empty_program()?;
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &empty)?;
    }
    Ok(game)
}

fn empty_program() -> Result<WellFormedCardProgram> {
    WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![],
    })
    .map_err(fixture_error)
}

fn fixture_error(error: impl Display) -> EngineFailure {
    EngineFailure::Invalid(format!("board fixture: {error}"))
}
