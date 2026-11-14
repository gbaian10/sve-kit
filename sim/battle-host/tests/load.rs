//! Session loading failures on the smallest inputs that reach each stage.

use serde_json::json;
use sha2::{Digest as _, Sha256};
use sve_battle_host::load::{Inputs, LoadError, load};

const KEYWORDS: &str = r#"{"format":1,"kind":"keyword_registry","keywords":{}}"#;
const EMPTY_SELECTION: &str = r#"{"documents":[],"authors":[]}"#;

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| [byte >> 4_u8, byte & 15])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect()
}

/// One synthetic printing, so the rule resources have an anchor for their one evidence entry.
fn snapshot() -> String {
    json!({"number":"t-anchor","faces":[{"name":"t-anchor","card_class":"ニュートラル",
        "card_type":"フォロワー","cost":"1","power":"1","hp":"1","traits":[],"text":null,
        "sections":[],"title":"synthetic"}]})
    .to_string()
}

/// The smallest rule resources the schema and evidence closure accept for that snapshot.
fn rules(snapshot: &str) -> String {
    let source = format!("src:v1:{}", "a".repeat(64));
    json!({"format":1_u8,"kind":"engine_rules",
        "scope":{"region":"jp","rules_version":"synthetic-1","rules_source_version_id":source},
        "input":{"kind":"legacy-jp","snapshot_sha256":hex(snapshot.as_bytes())},
        "evidence":[{"id":"synthetic","source_version_id":source,"rule_refs":["1.2.3"],
            "checked_on":"2026-10-03"}],
        "titles":[{"title_code":"synthetic_empty","capabilities":[],"evidence":["synthetic"],
            "anchor":{"kind":"legacy-face","region":"jp","card_no":"t-anchor","face_ordinal":0_u8}}],
        "resources":[]})
    .to_string()
}

fn setup(card: &str) -> String {
    let deck = json!({"construction":{"kind":"class","value":"ニュートラル"},
        "main":vec![card; 8],"evolve":[]});
    json!({"decks":[deck, deck],"first_chooser":"p1","seed":"load","control":["player","policy"],
        "max_submissions":10_u8})
    .to_string()
}

fn error(selection: &str, rules: &str, setup: &str) -> Option<LoadError> {
    let snapshot = snapshot();
    let inputs = Inputs {
        snapshot: &snapshot,
        keywords: KEYWORDS,
        selection,
        rules,
    };
    load(inputs, setup).err()
}

#[test]
fn each_stage_reports_its_own_code() {
    let rules = rules(&snapshot());
    assert_eq!(
        error(EMPTY_SELECTION, &rules, "{}"),
        Some(LoadError::Format)
    );
    assert_eq!(error("[", &rules, &setup("x")), Some(LoadError::Format));
    assert_eq!(
        error(EMPTY_SELECTION, "{}", &setup("x")),
        Some(LoadError::Selection)
    );
    // The selection loads, but the deck names a card outside it.
    assert_eq!(
        error(EMPTY_SELECTION, &rules, &setup("NOT-A-CARD")),
        Some(LoadError::Match)
    );
    assert_eq!(
        [LoadError::Format, LoadError::Selection, LoadError::Match].map(LoadError::code),
        ["format", "selection", "match"]
    );
}
