//! Session loading failures on the smallest inputs that reach each stage.

#![expect(
    clippy::unwrap_used,
    reason = "A malformed synthetic fixture invalidates these tests."
)]

#[path = "support/fixture.rs"]
mod fixture;

use serde_json::{Value, json};
use sve_battle_contract::{HostMessage, Identity};
use sve_battle_host::load::{LoadError, PreparedBattle, load};
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
    let no_manifest = references(|input| input.rules = None);
    for (label, references) in [
        ("a face outside the decks", unused_missing),
        ("another snapshot", wrong_snapshot),
        ("another region", wrong_region),
        ("an unknown printing", unknown_printing),
        ("rules of another manifest", rules_of_another_manifest),
        (
            "neither wording nor rules from the pinned manifest",
            no_manifest,
        ),
    ] {
        assert_eq!(
            error(EMPTY_SELECTION, &rules, &references, &setup("t-anchor")),
            Some(LoadError::References),
            "{label}"
        );
    }
}

#[test]
fn a_published_program_opens_a_deck_by_permanent_printing() {
    let native = snapshot();
    let card = format!("c:{}", "a".repeat(32));
    let face = format!("f:{}", "b".repeat(32));
    let unused = format!("f:{}", "c".repeat(32));
    let mut rules: Value = serde_json::from_str(&fixture::rules(&native, "t-anchor")).unwrap();
    rules["input"]["kind"] = json!("resolved");
    rules["titles"][0_usize]["anchor"] = json!({"kind":"face-id","region":"jp","face_id":face});
    let rules = rules.to_string();
    let mut refs = fixture::references(&native);
    refs.wording = Some(
        serde_json::from_value(json!({
            "manifest_sha256": format!("sha256:{}", fixture::MANIFEST),
            "faces": [
                {"card_no":"t-anchor","native_face":0_u8,"face_id":face,
                    "revisions":[{"revision_id":"revision","fields":[]}]},
                {"card_no":"t-unused","native_face":0_u8,"face_id":unused,
                    "revisions":[{"revision_id":"unused","fields":[]}]}
            ]
        }))
        .unwrap(),
    );
    let programs = json!({
        "format_version":"4.2.0",
        "entries": [{"id":"synthetic","dsl_version":"1.0",
            "ast":{"card_id":card,"scope":"shared","faces":{(face.clone()):{"abilities":[]}}}}],
        "printings": {(card.clone()):{"representative":"test-t-anchor","tokens":[]}}
    })
    .to_string();
    let identity = fixture::identity(&programs, &rules);
    let mut document = json!({
        "snapshot":native,"keywords":KEYWORDS,"references":refs,"rules":rules,"identity":identity,
        "engine_identity":{"Resolved":{"region":"jp","snapshot_sha256":refs.snapshot_sha256,
            "faces":[{"card_no":"t-anchor","face_ordinal":0_u8,"face_id":face,
                "rules_name_id":"synthetic-name","title_code":"synthetic_empty"}]}},
        "printings":[
            {"printing_id":"test-t-anchor","card_no":"t-anchor","card_id":card,
                "faces":[{"face_id":face,"revision_id":"revision","cost":1_u8,"rules_name":"t-anchor"}]},
            {"printing_id":"test-t-unused","card_no":"t-unused","card_id":format!("c:{}", "c".repeat(32)),
                "faces":[{"face_id":unused,"revision_id":"unused","cost":1_u8,"rules_name":"t-unused"}]}
        ],
    });
    let prepared = PreparedBattle::new(&document.to_string(), &programs).unwrap();
    let opening = prepared.open(&setup("test-t-anchor")).unwrap().start();
    assert!(
        opening
            .iter()
            .any(|delivery| matches!(delivery.message, HostMessage::Ready(_)))
    );
    assert_eq!(
        prepared.open(&setup("test-t-unused")).err(),
        Some(LoadError::Match)
    );
    document["printings"][0_usize]["faces"][0_usize]["revision_id"] = json!("foreign");
    assert_eq!(
        PreparedBattle::new(&document.to_string(), &programs).err(),
        Some(LoadError::References)
    );
}
