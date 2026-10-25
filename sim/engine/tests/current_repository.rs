//! Migrated repository cards stay complete D0 candidates, separate from runtime admission.

#![cfg(feature = "runner")]

use serde_json::Value;
use std::{env::var_os, fs::read_to_string, path::PathBuf};
use sve_engine::{catalog::Catalog, ir::lowering::ExpandedLowerer};

#[test]
fn repository_lastword_keeps_its_permanent_source_and_complete_damage_body() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
    let input: Value = serde_json::from_str(
        &read_to_string(var_os("SVE_TEST_CURRENT_SELECTION").unwrap()).unwrap(),
    )
    .unwrap();
    let keywords = read_to_string(root.join("authored/rules/keywords.yaml")).unwrap();
    let selected = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    let prototypes = serde_json::to_value(&selected.prototypes).unwrap();
    assert!(prototypes["programs"].get("BP01-P25").is_none());
    let card = &selected.current["BP01-P25"];
    assert_eq!(card.card_id, "c:efb62b3ef53e576797625688d75164d9");
    assert_eq!(card.program.card().abilities.len(), 1);
    let author = input["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "BP01-P25")
        .unwrap();
    let direct = ExpandedLowerer::new()
        .unwrap()
        .lower_current(&author["input"])
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
