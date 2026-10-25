//! Formal current selections preserve provenance through whole-card execution.

#![cfg(feature = "runner")]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Shared fixture readers fail immediately on a missing or malformed input."
)]

use serde_json::{Value, json};
use std::{env::var_os, fs::read_to_string, path::PathBuf};
use sve_engine::{EngineFailure, catalog::Catalog, ir::lowering::ExpandedLowerer};

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
    let prototypes = serde_json::to_value(&*selected.prototypes).unwrap();
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

#[path = "current_repository/runtime.rs"]
mod runtime;

#[test]
fn repository_draw_cards_lower_as_complete_current_candidates() {
    let (snapshot, keywords, input) = inputs();
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let prototypes = serde_json::to_value(&selected.prototypes).unwrap();
    let cases = [
        (
            "BP10-122",
            "c:9c92781995b55d629c700b5f25179da6",
            "attack_time",
            1_u8,
        ),
        (
            "CP02-068",
            "c:0850c7b7bda15bbaa0cbd019e7fba60c",
            "spell",
            3_u8,
        ),
        (
            "ECP01-002",
            "c:d02c2e5925ce5e7e94d6d7b3ea6be009",
            "evolve_time",
            1_u8,
        ),
    ];

    for (card_no, card_id, head, count) in cases {
        assert!(prototypes["programs"].get(card_no).is_none());
        let card = &selected.current[card_no];
        assert_eq!(card.card_id, card_id);
        assert_eq!(card.program.card().abilities.len(), 1);
        assert_eq!(
            card.expanded["cards"][card_no]["abilities"][0][head]["do"][0]["draw"],
            json!({"who":"self", "n":count})
        );
        let source = card
            .source_at(&format!("/cards/{card_no}/abilities/0/{head}/do/0/draw/n"))
            .unwrap();
        assert_eq!(source.source.card_id, card_id);
        assert_eq!(source.source.card_no, card_no);
        assert_eq!(source.source.scope, "ability_0");
        assert_eq!(source.source.field, "effect");
        assert_eq!(source.source.line, 1);
        assert_eq!(source.source.section, None);
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
        let failure = lowerer.lower_current(&candidate).err().unwrap();
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
        let selection = Catalog::from_current_selection(&snapshot, &keywords, &rejected)
            .err()
            .unwrap();
        assert!(matches!(&selection, EngineFailure::Invalid(reason)
            if reason == &format!("{code} at {path}")));
    }
}

#[path = "current_repository/evolution.rs"]
mod evolution;
