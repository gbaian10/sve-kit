//! Printed-card regression tests for fixes that no shared scenario covers
//! (docs/m0/known-errors.md KE-13, KE-14). Needs `SVE_TEST_SNAPSHOT`.

#![cfg(feature = "runner")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "A missing fixture or a construction error must fail the test."
)]

extern crate alloc;

use alloc::sync::Arc;
use std::env::var_os;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::{Value, json};
use sve_engine::catalog::Catalog;
use sve_engine::game::Game;

fn catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let snapshot = PathBuf::from(
            var_os("SVE_TEST_SNAPSHOT")
                .expect("set SVE_TEST_SNAPSHOT to the immutable cards.jsonl input"),
        );
        Arc::new(Catalog::load(&snapshot, &root.join("authored")).unwrap())
    }))
}

fn position(class: &str, hand: &Value, deck: &Value, counters: &Value) -> Value {
    json!({
        "turn":{"active":"P1","first_player":"P1","elapsed_turns":{"P1":6,"P2":5},"phase":"main"},
        "history":"explicit",
        "players":{
            "P1":{"leader":{"class":class,"life":20},"pp":{"current":10,"max":10},"ep":0,"sep":0,
                  "construction":"class","zones":{"hand":hand,"deck":deck}},
            "P2":{"leader":{"class":class,"life":20},"pp":{"current":10,"max":10},"ep":0,"sep":0,
                  "construction":"class","zones":{"deck":[{"id":"p2","card":"BP01-173"}]}}
        },
        "semantic_state":{"counters_this_turn":counters}
    })
}

/// KE-13, BP16-036: the selection limit reads the evolution counter.
/// counts every evolution (`evolutions`, also effect evolutions, Q2084), not only the
/// evolve actions (`evolve_played`).
#[test]
fn bp16_036_widens_the_choice_after_any_evolution() {
    let max_options = |counters: &Value| {
        let game = Game::new(
            catalog(),
            &position(
                "ロイヤル",
                &json!([{"id":"s","card":"BP16-036"}]),
                &json!([{"id":"d","card":"BP01-173"}]),
                counters,
            ),
            &Value::Null,
            &Value::Null,
            "bp16-036",
        )
        .unwrap();
        game.legal()
            .unwrap()
            .iter()
            .filter(|choice| choice["card"] == "s")
            .map(|choice| choice["options"].as_array().map_or(0, Vec::len))
            .max()
            .unwrap()
    };
    assert_eq!(max_options(&json!({})), 1);
    assert_eq!(
        max_options(&json!({"P1.evolve_played":1_i64,"P1.evolutions":0_i64})),
        1
    );
    assert_eq!(max_options(&json!({"P1.evolutions":1_i64})), 2);
}

/// KE-14, BP20-P28: only onmyoji followers and spells are offered.
#[test]
fn bp20_p28_offers_onmyoji_followers_or_spells_only() {
    let mut game = Game::new(
        catalog(),
        &position(
            "ウィッチ",
            &json!([{"id":"s","card":"BP20-P28"},{"id":"h","card":"BP01-173"}]),
            &json!([{"id":"f","card":"BP20-P28"},{"id":"sp","card":"BP21-012"},
                    {"id":"am","card":"BP21-P21"},{"id":"rest","card":"BP01-173"}]),
            &json!({}),
        ),
        &Value::Null,
        &Value::Null,
        "bp20-p28",
    )
    .unwrap();
    game.decide(&json!({"do":"play","card":"s"}), "play")
        .unwrap();
    let pending = game
        .legal()
        .unwrap()
        .into_iter()
        .find(|choice| choice["do"] == "choose-pending" && choice["costs"]["1"] == json!(["h"]))
        .unwrap();
    game.decide(&pending, "fanfare").unwrap();
    let mut offered = game
        .legal()
        .unwrap()
        .iter()
        .filter_map(|choice| choice["select"].as_array().cloned())
        .flatten()
        .filter_map(|id| id.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    offered.sort();
    assert_eq!(offered, ["f", "sp"]);
}
