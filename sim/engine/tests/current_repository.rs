//! Migrated repository cards stay complete D0 candidates, separate from runtime admission.

#![cfg(feature = "runner")]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Shared fixture readers fail immediately on a missing or malformed input."
)]

extern crate alloc;
use alloc::sync::Arc;
use serde_json::{Value, json};
use std::{env::var_os, fs::read_to_string, path::PathBuf};
use sve_engine::{
    EngineFailure,
    catalog::Catalog,
    game::{Game, View},
    ir::lowering::ExpandedLowerer,
};

fn inputs() -> (String, String, Value) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
    let input: Value = serde_json::from_str(
        &read_to_string(var_os("SVE_TEST_CURRENT_SELECTION").unwrap()).unwrap(),
    )
    .unwrap();
    let keywords = read_to_string(root.join("authored/rules/keywords.yaml")).unwrap();
    (snapshot, keywords, input)
}

fn p25(input: &Value) -> &Value {
    &input["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "BP01-P25")
        .unwrap()["input"]
}

#[test]
fn repository_lastword_keeps_its_permanent_source_and_complete_damage_body() {
    let (snapshot, keywords, input) = inputs();
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let prototypes = serde_json::to_value(&selected.prototypes).unwrap();
    assert!(prototypes["programs"].get("BP01-P25").is_none());
    let card = &selected.current["BP01-P25"];
    assert_eq!(card.card_id, "c:efb62b3ef53e576797625688d75164d9");
    assert_eq!(card.program.card().abilities.len(), 1);
    let direct = ExpandedLowerer::new()
        .unwrap()
        .lower_current(p25(&input))
        .unwrap();
    assert_eq!(card.source_version, direct.source_version);
    assert_eq!(card.source_payload["token_links"], serde_json::json!([]));
    let ability = &card.expanded["cards"]["BP01-P25"]["abilities"][0];
    assert_eq!(
        ability["lastword"],
        serde_json::json!({"do":[{"damage":{"to":{"leaders":"all"},"n":2_u8}}]})
    );
    let source = card
        .source_at("/cards/BP01-P25/abilities/0/lastword/do/0/damage/n")
        .unwrap();
    assert_eq!(source.source.face, "f:90c74424f2435907a035f70ce37463d4");
    assert_eq!(source.source.field, "effect");
    assert_eq!(source.source.line, 1);
    assert_eq!(source.source.section, None);
}

#[test]
fn repository_lastword_current_identity_is_blocked_at_whole_card_admission() {
    let (snapshot, keywords, input) = inputs();
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let program = &selected.current["BP01-P25"].program;
    let catalog = Arc::new(selected.prototypes);
    for seat in ["P1", "P2"] {
        let mut initial = json!({
            "history":"explicit",
            "turn":{"active":seat,"first_player":"P1","phase":"main",
                "elapsed_turns":{"P1":3_u8,"P2":3_u8}},
            "players":{
                "P1":{"leader":{"class":"ニュートラル","life":20_u8},
                    "pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}},
                "P2":{"leader":{"class":"ニュートラル","life":20_u8},
                    "pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}}
            }
        });
        initial["players"][seat]["zones"]["field"] = json!([{"id":"departing","card":"BP01-P25"}]);
        let mut game = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "E-B2-admission",
        )
        .unwrap();
        let before = format!("{game:?}");
        // This records the real D0/D1 barrier, not Lastword execution evidence.
        let admission = game.load_internal_card("BP01-P25", program).unwrap_err();
        assert!(matches!(&admission, EngineFailure::Unsupported(reason)
            if reason == "no authored program: BP01-P25"));
        assert_eq!(format!("{game:?}"), before);
        assert_eq!(game.internal_card_stage(), None);
        for view in [View::P1, View::P2, View::Referee] {
            let projection = game.projection(view).unwrap_err();
            assert!(matches!(&projection, EngineFailure::Unsupported(reason)
                if reason == "no authored program: BP01-P25"));
        }
        assert_eq!(format!("{game:?}"), before);
    }
}

#[test]
fn repository_lastword_rejects_missing_sources_and_unknown_or_unsupported_steps() {
    let (snapshot, keywords, input) = inputs();
    let lowerer = ExpandedLowerer::new().unwrap();
    let mut missing = p25(&input).clone();
    missing["sidecar"] = json!([]);
    let mut unknown = p25(&input).clone();
    unknown["tree"]["cards"]["BP01-P25"]["abilities"][0]["lastword"]["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"unknown_step":{}}));
    let mut unsupported = p25(&input).clone();
    unsupported["tree"]["cards"]["BP01-P25"]["abilities"][0]["lastword"]["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"banish":"this"}));
    // Mutated handoffs are rejection probes, never accepted source evidence.
    for (candidate, code, path) in [
        (missing, "invalid_current_source", "/cards/BP01-P25"),
        (
            unknown,
            "unsupported_construct",
            "/cards/BP01-P25/abilities/0",
        ),
        (
            unsupported,
            "unsupported_construct",
            "/cards/BP01-P25/abilities/0",
        ),
    ] {
        let failure = lowerer.lower_current(&candidate).unwrap_err();
        assert_eq!(failure.code, code);
        assert_eq!(failure.path, path);
        let mut rejected = input.clone();
        let author = rejected["authors"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|author| author["input"]["card_no"] == "BP01-P25")
            .unwrap();
        author["input"] = candidate;
        let selection =
            Catalog::from_current_selection(&snapshot, &keywords, &rejected).unwrap_err();
        assert!(matches!(&selection, EngineFailure::Invalid(reason)
            if reason == &format!("{code} at {path}")));
    }
}
