//! Historical regression catalogue from carddb's temporary public-input projection.
//!
//! Cards migrated to author 1.0 leave the prototype set, but the shared scenarios still
//! pin their original behaviour. This catalogue restores each migrated card's frozen
//! prototype AST for those scenarios only; it is never a current-selection result and
//! grants no runtime qualification (see `current_repository.rs` for the real selection).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use std::ffi::OsStr;
use std::fs::{read_dir, read_to_string};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::Value;
use sve_engine::catalog::{Catalog, EngineIdentityInput};

#[path = "repository.rs"]
mod repository;
#[path = "rule_fixtures.rs"]
mod rule_fixtures;

#[expect(
    clippy::expect_used,
    reason = "Missing generated input must fail every test scope."
)]
fn restore_frozen_programs(documents: &mut Vec<(String, String)>, root: &Path) {
    let migrated = root.join("carddb/tests/fixtures/effect_baseline/migrated");
    for entry in read_dir(migrated).expect("migrated baseline directory") {
        let path = entry.expect("migrated baseline entry").path();
        let number = path
            .file_stem()
            .and_then(OsStr::to_str)
            .expect("card number file name");
        let pack = number.split('-').next().expect("pack prefix");
        let program: Value = serde_json::from_str(&read_to_string(&path).expect("frozen program"))
            .expect("frozen program JSON");
        // The last live prototype can migrate out of a pack too.
        if !documents
            .iter()
            .any(|(name, _)| Path::new(name).file_stem().is_some_and(|stem| stem == pack))
        {
            documents.push((
                format!("historical/{pack}.yaml"),
                serde_json::json!({"format":1_u8, "kind":"effect_set", "cards":{}}).to_string(),
            ));
        }
        let (_, text) = documents
            .iter_mut()
            .find(|(name, _)| Path::new(name).file_stem().is_some_and(|stem| stem == pack))
            .expect("prototype document of the migrated card's pack");
        let mut document: Value = serde_saphyr::from_str(text).expect("prototype document");
        let replaced = document
            .get_mut("cards")
            .and_then(Value::as_object_mut)
            .expect("cards map")
            .insert(number.to_owned(), program);
        assert!(replaced.is_none(), "{number} is still a current prototype");
        *text = serde_json::to_string(&document).expect("document JSON");
    }
    restore_legacy_bp15_118(documents);
}

#[expect(
    clippy::expect_used,
    reason = "Historical scenarios require their original prototype program."
)]
fn restore_legacy_bp15_118(documents: &mut [(String, String)]) {
    let (_, text) = documents
        .iter_mut()
        .find(|(name, _)| {
            Path::new(name)
                .file_stem()
                .is_some_and(|stem| stem == "BP15")
        })
        .expect("historical BP15 prototype document");
    let mut document: Value = serde_saphyr::from_str(text).expect("prototype document");
    let program = serde_json::json!({
        "status":"complete",
        "abilities":[{"line":1_u8, "kind":"spell", "body":{"op":"seq", "steps":[
            {"op":"damage", "subjects":"target.1", "amount":{
                "if":{"fn":"ge", "args":[{"count":{"side":"self", "zone":"evolve_deck", "type":"follower", "where":{"read":"item.face_up"}}},3_u8]},
                "then":4_u8, "else":2_u8}},
            {"op":"if", "condition":{"fn":"ge", "args":[{"count":{"side":"self", "zone":"evolve_deck", "type":"follower", "where":{"read":"item.face_up"}}},5_u8]},
                "then":{"op":"seq", "steps":[{"op":"draw", "count":1_u8},{"op":"recover_pp", "amount":1_u8}]}}
        ]}, "targets":[{"key":"1", "select":{"side":"opponent", "zone":"field", "type":"follower"}, "min":1_u8, "max":1_u8}]}]
    });
    let replaced = document
        .get_mut("cards")
        .and_then(Value::as_object_mut)
        .expect("cards map")
        .insert("BP15-118".to_owned(), program);
    assert!(replaced.is_none(), "BP15-118 is still a current prototype");
    *text = serde_json::to_string(&document).expect("document JSON");
}

/// Selected prototype documents with their original line layout, plus the shared inputs.
#[expect(
    clippy::expect_used,
    reason = "Missing generated input must fail every test scope."
)]
fn selected_prototypes() -> (&'static str, &'static str, Vec<(String, String)>, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (snapshot, keywords, input) = repository::inputs();
    // Accept the real selection before restoring any historical program.
    repository::selection();
    let documents: Vec<(String, String)> = input
        .get("documents")
        .and_then(Value::as_array)
        .expect("selected prototype documents")
        .iter()
        .map(|pair| {
            let pair = pair.as_array().expect("document pair");
            (
                pair.first()
                    .and_then(Value::as_str)
                    .expect("document path")
                    .to_owned(),
                pair.get(1)
                    .and_then(Value::as_str)
                    .expect("document text")
                    .to_owned(),
            )
        })
        .collect();
    (snapshot, keywords, documents, root)
}

#[expect(
    clippy::expect_used,
    reason = "Missing generated input must fail every test scope."
)]
fn build(snapshot: &str, keywords: &str, documents: &[(String, String)], root: &Path) -> Catalog {
    let rules_source =
        read_to_string(root.join("authored/rules/engine/index.yaml")).expect("rule bindings");
    let mut rules_document: Value =
        serde_saphyr::from_str(&rules_source).expect("repository rules");
    // Batch upgrades change snapshot bytes without changing the historical rule capabilities.
    *rules_document
        .pointer_mut("/input/snapshot_sha256")
        .expect("rule snapshot binding") = Value::String(rule_fixtures::hash(snapshot.as_bytes()));
    Catalog::from_documents_with_rules(
        snapshot,
        keywords,
        documents,
        &rules_document.to_string(),
        &EngineIdentityInput::LegacyJp,
    )
    .expect("selected prototypes with rule bindings")
}

#[expect(
    clippy::expect_used,
    reason = "Historical passive token fixtures use the checked current author identity."
)]
fn include_passive_current_amulets(
    documents: &mut Vec<(String, String)>,
    input: &Value,
    root: &Path,
    snapshot: &str,
) {
    let available: BTreeSet<String> = snapshot
        .lines()
        .filter_map(|line| {
            let card: Value = serde_json::from_str(line).expect("snapshot card");
            if card.pointer("/faces/0/card_type").and_then(Value::as_str)
                != Some("アミュレット・トークン")
            {
                return None;
            }
            Some(
                card.get("number")
                    .and_then(Value::as_str)
                    .expect("snapshot printing")
                    .to_owned(),
            )
        })
        .collect();
    let mut tokens = BTreeMap::new();
    for author in input["authors"].as_array().expect("current authors") {
        if author["status"] != "accepted" {
            continue;
        }
        let candidate = &author["input"];
        let number = candidate["card_no"].as_str().expect("current card number");
        let token = candidate
            .get("tree")
            .and_then(|tree| tree.get("cards"))
            .and_then(|cards| cards.get(number))
            .and_then(|card| card.get("token"));
        if let Some(token) = token.filter(|token| token.is_object()) {
            tokens.insert(
                candidate["card_id"]
                    .as_str()
                    .expect("current card identity"),
                (number, token),
            );
        }
    }
    let existing: BTreeSet<String> = documents
        .iter()
        .flat_map(|(_, text)| {
            let document: Value = serde_saphyr::from_str(text).expect("historical document");
            document
                .get("cards")
                .and_then(Value::as_object)
                .expect("historical cards")
                .keys()
                .cloned()
                .collect::<Vec<_>>()
        })
        .collect();
    let mut by_identity: BTreeMap<String, serde_json::Map<String, Value>> = BTreeMap::new();
    let mut historical = BTreeSet::new();
    for pack in read_dir(root.join("authored/registry/printing")).expect("printing registry") {
        for entry in read_dir(pack.expect("printing pack").path()).expect("printing pack files") {
            let path = entry.expect("printing registry file").path();
            let registry: Value =
                serde_saphyr::from_str(&read_to_string(path).expect("printing registry input"))
                    .expect("printing registry YAML");
            for record in registry
                .get("records")
                .and_then(Value::as_array)
                .expect("printing records")
            {
                let data = &record["data"];
                if record["kind"] != "printing" || data["region"] != "jp" {
                    continue;
                }
                let Some((canonical, token)) =
                    data["card_id"].as_str().and_then(|id| tokens.get(id))
                else {
                    continue;
                };
                let number = data["card_no"].as_str().expect("JP printing coordinate");
                let identity = data["card_id"].as_str().expect("printing identity");
                if existing.contains(number) {
                    historical.insert(identity.to_owned());
                }
                if !available.contains(number) {
                    continue;
                }
                // Shared legacy scenarios only use these amulets as passive payment material.
                // Full current abilities are exercised by current_repository, never this fixture.
                by_identity.entry(identity.to_owned()).or_default().insert(
                    number.to_owned(),
                    serde_json::json!({
                        "status": "complete", "abilities": [], "rules_name": token["name"],
                        "token_template": number == *canonical && token["template"] == true,
                    }),
                );
            }
        }
    }
    let cards: serde_json::Map<String, Value> = by_identity
        .into_iter()
        .filter(|(identity, _)| !historical.contains(identity))
        .flat_map(|(_, cards)| cards)
        .collect();
    documents.push((
        "historical/passive-current-tokens.yaml".to_owned(),
        serde_json::json!({"format":1_u8, "kind":"effect_set", "cards":cards}).to_string(),
    ));
}

/// Raw current prototype documents (no frozen restore), so load-time rejection findings
/// keep the original YAML line numbers.
#[allow(
    dead_code,
    reason = "Included by several test crates; only the rejection test needs it."
)]
pub(crate) fn current_prototype_catalog() -> Catalog {
    let (snapshot, keywords, documents, root) = selected_prototypes();
    build(snapshot, keywords, &documents, &root)
}

pub(crate) fn historical_regression_catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let (snapshot, keywords, mut documents, root) = selected_prototypes();
        restore_frozen_programs(&mut documents, &root);
        let (_, _, input) = repository::inputs();
        include_passive_current_amulets(&mut documents, input, &root, snapshot);
        Arc::new(build(snapshot, keywords, &documents, &root))
    }))
}
