//! Session loading failures on the smallest inputs that reach each stage.

#![expect(
    clippy::unwrap_used,
    reason = "A malformed synthetic fixture invalidates these tests."
)]

#[path = "support/fixture.rs"]
mod fixture;

use serde_json::json;
use sve_battle_contract::Identity;
use sve_battle_host::load::{LoadError, load};
use sve_engine::battle::{ReferenceInput, RuleIdentity, RuleInput};

const KEYWORDS: &str = r#"{"format":1,"kind":"keyword_registry","keywords":{}}"#;
const EMPTY_SELECTION: &str = r#"{"documents":[],"authors":[]}"#;

/// The anchor of the rule evidence and a printing no deck uses.
fn snapshot() -> String {
    ["t-anchor", "t-unused"]
        .map(|number| {
            json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル",
                "card_type":"フォロワー","cost":"1","power":"1","hp":"1","traits":[],"text":null,
                "sections":[],"title":"synthetic"}]})
            .to_string()
        })
        .join("\n")
}

fn setup(card: &str) -> String {
    let deck = json!({"construction":{"kind":"class","value":"ニュートラル"},
        "main":vec![card; 8],"evolve":[]});
    json!({"decks":[deck, deck],"first_chooser":"p1","seed":"load","control":["player","policy"],
        "max_submissions":10_u8})
    .to_string()
}

fn loaded(
    selection: &str,
    rules: &str,
    references: &str,
    identity: &Identity,
    setup: &str,
) -> Option<LoadError> {
    let snapshot = snapshot();
    let references = serde_json::from_str(references).unwrap();
    let inputs = fixture::document(
        [&snapshot, KEYWORDS, selection, rules],
        &references,
        identity,
    );
    load(&inputs, setup).err()
}

fn error(selection: &str, rules: &str, references: &str, setup: &str) -> Option<LoadError> {
    loaded(
        selection,
        rules,
        references,
        &fixture::identity(selection, rules),
        setup,
    )
}

fn rule_input(manifest: &str) -> RuleInput {
    RuleInput {
        manifest_sha256: format!("sha256:{manifest}"),
        entries: vec![RuleIdentity::Cr {
            id: "1.2.3".into(),
            revision: "synthetic".into(),
        }],
    }
}

fn references(edit: impl FnOnce(&mut ReferenceInput)) -> String {
    let mut input = fixture::references(&snapshot());
    edit(&mut input);
    serde_json::to_string(&input).unwrap()
}

#[test]
fn each_stage_reports_its_own_code() {
    let rules = fixture::rules(&snapshot(), "t-anchor");
    let exact = references(|_| ());
    assert_eq!(
        error(EMPTY_SELECTION, &rules, &exact, "{}"),
        Some(LoadError::Format)
    );
    assert_eq!(
        error("[", &rules, &exact, &setup("t-anchor")),
        Some(LoadError::Format)
    );
    assert_eq!(
        error(EMPTY_SELECTION, &rules, "{}", &setup("t-anchor")),
        Some(LoadError::Format)
    );
    let stated = fixture::identity("{}", &rules);
    assert_eq!(
        loaded(EMPTY_SELECTION, &rules, &exact, &stated, &setup("t-anchor")),
        Some(LoadError::Identity),
        "an identity must name exactly the programs and rules loaded"
    );
    assert_eq!(
        error(EMPTY_SELECTION, "{}", &exact, &setup("t-anchor")),
        Some(LoadError::Selection)
    );
    // Exact references pass; the deck card has no admitted effects author.
    assert_eq!(
        error(EMPTY_SELECTION, &rules, &exact, &setup("t-anchor")),
        Some(LoadError::Match)
    );
    assert_eq!(
        [
            LoadError::Format,
            LoadError::Selection,
            LoadError::Identity,
            LoadError::References,
            LoadError::Match
        ]
        .map(LoadError::code),
        ["format", "selection", "identity", "references", "match"]
    );
}

#[test]
fn references_must_bind_exactly_this_whole_snapshot() {
    let rules = fixture::rules(&snapshot(), "t-anchor");
    let unused_missing = references(|input| input.faces.retain(|face| face.card_no != "t-unused"));
    let wrong_snapshot = references(|input| input.snapshot_sha256 = "0".repeat(64));
    let wrong_region = references(|input| input.region = "en".into());
    let unknown_printing = references(|input| {
        let mut absent = input.faces.first().unwrap().clone();
        absent.card_no = "t-absent".into();
        input.faces.push(absent);
    });
    let rules_of_another_manifest =
        references(|input| input.rules = Some(rule_input(&"a".repeat(64))));
    for (label, references) in [
        ("a face outside the decks", unused_missing),
        ("another snapshot", wrong_snapshot),
        ("another region", wrong_region),
        ("an unknown printing", unknown_printing),
        ("rules of another manifest", rules_of_another_manifest),
    ] {
        assert_eq!(
            error(EMPTY_SELECTION, &rules, &references, &setup("t-anchor")),
            Some(LoadError::References),
            "{label}"
        );
    }
}

#[test]
fn rules_without_wording_must_come_from_the_pinned_manifest() {
    let rules = fixture::rules(&snapshot(), "t-anchor");
    let pinned = references(|input| input.rules = Some(rule_input(fixture::MANIFEST)));
    // The references pass; the deck card has no admitted effects author.
    assert_eq!(
        error(EMPTY_SELECTION, &rules, &pinned, &setup("t-anchor")),
        Some(LoadError::Match)
    );
}
