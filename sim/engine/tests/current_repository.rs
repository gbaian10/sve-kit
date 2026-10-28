//! Formal current selections preserve provenance through whole-card execution.

#![cfg(feature = "runner")]
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "Shared fixture readers fail immediately on a missing or malformed input."
)]

extern crate alloc;

use serde_json::{Value, json};
use std::sync::OnceLock;
use sve_engine::game::Game;
use sve_engine::{EngineFailure, catalog::Catalog};

const BP06_048_FACE: &str = "f:e9d8deca215953a99d12bd07acd566fa";

#[path = "support/repository.rs"]
mod repository;

#[expect(
    dead_code,
    reason = "Runtime tests use only the evolution expansion fixture."
)]
#[path = "support/expanded.rs"]
mod expanded;
#[path = "support/rule_fixtures.rs"]
mod rule_fixtures;

use repository::{inputs, lowerer, selection, selection_with_rules};

const BP01_P25_FACE: &str = "f:90c74424f2435907a035f70ce37463d4";

fn p25(input: &Value) -> &Value {
    &input["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "BP01-P25")
        .unwrap()["input"]
}

fn b4(input: &Value) -> &Value {
    &input["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "BP06-048")
        .unwrap()["input"]
}

#[test]
fn repository_lastword_keeps_its_permanent_source_and_complete_damage_body() {
    let selected = selection().clone();
    let prototypes = serde_json::to_value(&*selected.prototypes).unwrap();
    assert!(prototypes["programs"].get("BP01-P25").is_none());
    let card = &selected.current["BP01-P25"];
    assert_eq!(card.card_id, "c:efb62b3ef53e576797625688d75164d9");
    assert_eq!(card.program.card().abilities.len(), 1);
    let direct = lowerer().lower_current(p25(&inputs().2)).unwrap();
    assert_eq!(card.source_version, direct.source_version);
    assert_eq!(card.source_payload["card_id"], card.card_id);
    assert_eq!(card.source_payload["faces"].as_array().unwrap().len(), 1);
    assert_eq!(card.source_payload["faces"][0]["face_id"], BP01_P25_FACE);
    let ability = &card.expanded["cards"]["BP01-P25"]["faces"][BP01_P25_FACE]["abilities"][0];
    assert_eq!(
        ability["lastword"],
        serde_json::json!({"do":[{"damage":{"to":{"leaders":"all"},"n":2_u8}}]})
    );
    let source = card
        .source_at(&format!(
            "/cards/BP01-P25/faces/{BP01_P25_FACE}/abilities/0/lastword/do/0/damage/n"
        ))
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
    let selected = selection().clone();
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
fn repository_keyword_migrations_lower_as_complete_current_candidates() {
    let selected = selection().clone();
    let prototypes = serde_json::to_value(&selected.prototypes).unwrap();
    let cases = [("DSD01b-004", "c:31103e7d8c6b5a2d9e3838cac19b1c07", 2)];

    for (card_no, card_id, abilities) in cases {
        assert!(prototypes["programs"].get(card_no).is_none());
        let card = &selected.current[card_no];
        assert_eq!(card.card_id, card_id);
        assert_eq!(card.program.card().abilities.len(), abilities);
    }

    for card_no in [
        "BP01-096", "BP01-158", "BP01-163", "BP01-P10", "BP04-P07", "BP03-033", "BP14-032",
    ] {
        assert!(!selected.current.contains_key(card_no));
        assert!(prototypes["programs"].get(card_no).is_some());
    }
}

#[test]
fn repository_named_search_lowers_as_a_complete_current_candidate() {
    let selected = selection().clone();
    let prototypes = serde_json::to_value(&selected.prototypes).unwrap();
    let card_no = "BP06-048";
    let card_id = "c:e7b93196e1175515a42771a72c7b12da";
    let face_id = "f:e9d8deca215953a99d12bd07acd566fa";

    assert!(prototypes["programs"].get(card_no).is_none());
    let card = &selected.current[card_no];
    assert_eq!(card.card_id, card_id);
    assert_eq!(card.program.card().abilities.len(), 1);
    assert_eq!(
        card.expanded["cards"][card_no]["faces"][face_id]["abilities"][0]["lastword"]["do"][0]["search"],
        json!({"sel":{"who":"self","zone":"deck","name":"式神の使役"},"n":1_i32,"to":"hand"})
    );
    let direct = lowerer().lower_current(b4(&inputs().2)).unwrap();
    assert_eq!(card.source_version, direct.source_version);
    let source = card
        .source_at(&format!(
            "/cards/{card_no}/faces/{face_id}/abilities/0/lastword/do/0/search/n"
        ))
        .unwrap();
    assert_eq!(source.source.card_id, card_id);
    assert_eq!(source.source.card_no, card_no);
    assert_eq!(source.source.face, face_id);
    assert_eq!(source.source.scope, "ability_0");
    assert_eq!(source.source.field, "effect");
    assert_eq!(source.source.line, 1);
    assert_eq!(source.source.section, None);
}

#[test]
fn repository_named_search_rejects_wrong_owner_missing_source_and_incomplete_closure() {
    let input = &inputs().2;
    let lowerer = lowerer();

    let mut wrong_owner = b4(input).clone();
    wrong_owner["tree"]["cards"]["BP06-048"]["faces"][BP06_048_FACE]["abilities"][0]["lastword"]
        ["do"][0]["search"]["sel"]["who"] = json!("opponent");
    let owner_failure = lowerer.lower_current(&wrong_owner).err().unwrap();
    assert_eq!(owner_failure.code, "unsupported_construct");
    assert_eq!(
        owner_failure.path,
        format!("/cards/BP06-048/faces/{BP06_048_FACE}/abilities/0")
    );

    let mut missing_source = b4(input).clone();
    missing_source["source_payload"]["faces"] = json!([]);
    let source_failure = lowerer.lower_current(&missing_source).err().unwrap();
    assert_eq!(source_failure.code, "invalid_direct_admission");
    assert_eq!(source_failure.path, "/card_id");

    let mut incomplete = selection().clone();
    incomplete.current.remove("BP06-048");
    let error = Game::from_selected_effects(
        incomplete,
        &runtime::setup("P1", "P1", [20, 20]),
        &Value::Null,
        &Value::Null,
        "incomplete-bp06-selection",
    )
    .err()
    .unwrap();
    assert!(
        matches!(&error, EngineFailure::Invalid(reason)
            if reason == "current selection changed after acceptance"),
        "the whole current Game rejects a closure with BP06-048 removed"
    );
}

#[test]
fn repository_lastword_rejects_missing_sources_and_unknown_or_unsupported_steps() {
    let (snapshot, keywords, input) = inputs();
    let lowerer = lowerer();
    let mut missing = p25(input).clone();
    missing["source_payload"]["faces"] = json!([]);
    let mut unknown = p25(input).clone();
    unknown["tree"]["cards"]["BP01-P25"]["faces"][BP01_P25_FACE]["abilities"][0]["lastword"]["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"unknown_step":{}}));
    let mut unsupported = p25(input).clone();
    unsupported["tree"]["cards"]["BP01-P25"]["faces"][BP01_P25_FACE]["abilities"][0]["lastword"]["do"]
        .as_array_mut().unwrap().push(json!({"dice":{"bind":"roll"}}));
    // Mutations must reach the direct handoff's real inventory and face-qualified body.
    for (candidate, code, path, exact) in [
        (
            missing,
            "invalid_direct_admission",
            "/card_id".to_owned(),
            true,
        ),
        (
            unknown,
            "invalid_expanded",
            "/cards/BP01-P25".to_owned(),
            false,
        ),
        (
            unsupported,
            "unsupported_construct",
            format!("/cards/BP01-P25/faces/{BP01_P25_FACE}/abilities/0"),
            true,
        ),
    ] {
        let failure = lowerer.lower_current(&candidate).err().unwrap();
        assert_eq!(failure.code, code);
        if exact {
            assert_eq!(failure.path, path);
        } else {
            assert!(
                failure.path.starts_with(&path),
                "schema rejection identifies the malformed card"
            );
        }
        let mut rejected = input.clone();
        let author = rejected["authors"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|author| author["input"]["card_no"] == "BP01-P25")
            .unwrap();
        author["input"] = candidate;
        let selection = Catalog::from_current_selection(snapshot, keywords, &rejected)
            .err()
            .unwrap();
        assert!(matches!(&selection, EngineFailure::Invalid(reason)
            if reason == &format!("{code} at {}", failure.path)));
    }
}

#[test]
fn repository_selection_rejects_mixed_formats_and_never_falls_back() {
    let (snapshot, keywords, input) = inputs();
    let prototype = json!({
        "format": 1_u8,
        "kind": "effect_set",
        "cards": {
            "BP01-P25": {
                "status": "complete",
                "abilities": [{
                    "line": 1_u8,
                    "kind": "trigger",
                    "event": "field_to_cemetery",
                    "subject": "self",
                    "body": {"op": "damage", "subjects": "both.leaders", "amount": 2_u8}
                }]
            }
        }
    });
    let mut mixed = input.clone();
    mixed["documents"].as_array_mut().unwrap().push(json!([
        "probe.yaml",
        serde_json::to_string(&prototype).unwrap()
    ]));
    let prototype_only = json!({"documents": mixed["documents"], "authors": []});
    let available = Catalog::from_current_selection(snapshot, keywords, &prototype_only).unwrap();
    let programs = serde_json::to_value(&available.prototypes).unwrap();
    assert!(programs["programs"].get("BP01-P25").is_some());

    let overlap = Catalog::from_current_selection(snapshot, keywords, &mixed)
        .err()
        .unwrap();
    assert!(matches!(&overlap, EngineFailure::Invalid(reason)
        if reason == "current selection identity or prototype overlap"));

    let mut invalid = mixed.clone();
    let invalid_author = invalid["authors"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|author| author["input"]["card_no"] == "BP01-P25")
        .unwrap();
    invalid_author["input"]["source_payload"]["faces"] = json!([]);
    let failure = Catalog::from_current_selection(snapshot, keywords, &invalid)
        .err()
        .unwrap();
    assert!(matches!(&failure, EngineFailure::Invalid(reason)
        if reason == "invalid_direct_admission at /card_id"));

    let author = mixed["authors"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|author| author["input"]["card_no"] == "BP01-P25")
        .unwrap();
    *author = json!({
        "status": "rejected",
        "card_no": "BP01-P25",
        "reason": "source_pending"
    });
    let rejection = Catalog::from_current_selection(snapshot, keywords, &mixed)
        .err()
        .unwrap();
    assert!(matches!(&rejection, EngineFailure::Unsupported(reason)
        if reason == "current card rejected: BP01-P25: source_pending"));
}

#[path = "current_repository/evolution.rs"]
mod evolution;

#[path = "current_repository/advance.rs"]
mod advance;

#[test]
fn repository_selection_rejects_stale_anchors_without_prototype_fallback() {
    let (snapshot, keywords, mut input) = inputs().clone();
    let author = input["authors"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|author| author["input"]["card_no"] == "BP10-122")
        .unwrap();
    *author = json!({
        "status": "rejected",
        "card_no": "BP10-122",
        "reason": "stale_anchor"
    });
    let failure = Catalog::from_current_selection(&snapshot, &keywords, &input)
        .err()
        .unwrap();
    assert!(matches!(&failure, EngineFailure::Unsupported(reason)
        if reason == "current card rejected: BP10-122: stale_anchor"));
}
