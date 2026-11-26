//! Immutable public repository fixtures, initialized once per test binary.

#![allow(
    dead_code,
    reason = "Each test binary uses a different subset of the shared fixtures."
)]
#![expect(
    clippy::unwrap_used,
    reason = "Missing or malformed generated input must fail the tests."
)]

use super::rule_fixtures;

use serde_json::Value;
use std::{env::var_os, fs::read_to_string, path::PathBuf, sync::OnceLock};
use sve_battle_contract::{CardRef, Nat};
use sve_engine::battle::{BattleReferences, PrintingFaceIdentity, ReferenceInput};
use sve_engine::catalog::{CurrentSelectionLoader, SelectedEffects};
use sve_engine::game::Game;
use sve_engine::game::player::PlayerDeck;
use sve_engine::ir::lowering::ExpandedLowerer;

pub(crate) fn inputs() -> &'static (String, String, Value) {
    static INPUTS: OnceLock<(String, String, Value)> = OnceLock::new();
    INPUTS.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let snapshot = read_to_string(var_os("SVE_TEST_SNAPSHOT").unwrap()).unwrap();
        let input = serde_json::from_str(
            &read_to_string(var_os("SVE_TEST_CURRENT_SELECTION").unwrap()).unwrap(),
        )
        .unwrap();
        let keywords = read_to_string(root.join("authored/rules/keyword-registry.yaml")).unwrap();
        (snapshot, keywords, input)
    })
}

pub(crate) fn selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, input) = inputs();
        select(snapshot, keywords, input).unwrap()
    })
}

pub(crate) fn selection_with_rules() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, input) = inputs();
        selection_with_rules_from(snapshot, keywords, input)
    })
}

/// Wire printing identities for the JP repository test snapshot.
pub(crate) fn battle_references() -> &'static BattleReferences {
    static REFERENCES: OnceLock<BattleReferences> = OnceLock::new();
    REFERENCES.get_or_init(|| {
        let (snapshot, _, _) = inputs();
        let mut faces = Vec::new();
        for line in snapshot.lines() {
            let row: Value = serde_json::from_str(line).unwrap();
            let number = row.get("number").and_then(Value::as_str).unwrap();
            let count = row.get("faces").and_then(Value::as_array).unwrap().len();
            for ordinal in 0..count {
                faces.push(PrintingFaceIdentity {
                    card_no: number.into(),
                    native_face: u64::try_from(ordinal).unwrap(),
                    card: CardRef {
                        printing_id: format!("printing-{number}"),
                        face: Nat::try_from(ordinal).unwrap(),
                    },
                });
            }
        }
        BattleReferences::new(
            &selection_with_rules().catalogue,
            ReferenceInput {
                rules: None,
                wording: None,
                region: "jp".into(),
                snapshot_sha256: rule_fixtures::hash(snapshot.as_bytes()),
                faces,
            },
        )
        .unwrap()
    })
}

pub(crate) fn selection_with_rules_from(
    snapshot: &str,
    keywords: &str,
    input: &Value,
) -> SelectedEffects {
    select(snapshot, keywords, input).unwrap()
}

fn bound_rules(snapshot: &str) -> String {
    let rules_source = read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/engine/index.yaml"),
    )
    .unwrap();
    let mut rules_document: Value = serde_saphyr::from_str(&rules_source).unwrap();
    // Batch upgrades change snapshot bytes without changing these rule capabilities.
    rules_document
        .pointer_mut("/input")
        .and_then(Value::as_object_mut)
        .unwrap()
        .insert(
            "snapshot_sha256".into(),
            Value::String(rule_fixtures::hash(snapshot.as_bytes())),
        );
    serde_json::to_string(&rules_document).unwrap()
}

pub(crate) fn lowerer() -> &'static ExpandedLowerer {
    static LOWERER: OnceLock<ExpandedLowerer> = OnceLock::new();
    LOWERER.get_or_init(|| ExpandedLowerer::new().unwrap())
}

fn loader() -> &'static CurrentSelectionLoader {
    static LOADER: OnceLock<CurrentSelectionLoader> = OnceLock::new();
    LOADER.get_or_init(|| CurrentSelectionLoader::new().unwrap())
}

pub(crate) fn select(
    snapshot: &str,
    keywords: &str,
    input: &Value,
) -> sve_engine::Result<SelectedEffects> {
    // Full repository selections include cards whose admitted text depends on bound rule resources.
    select_with_rules(snapshot, keywords, input, &bound_rules(snapshot))
}

pub(crate) fn select_with_rules(
    snapshot: &str,
    keywords: &str,
    input: &Value,
    rules: &str,
) -> sve_engine::Result<SelectedEffects> {
    let (rules, identity) = rule_fixtures::resolved(snapshot, rules)?;
    loader().select_with_rules(snapshot, keywords, input, &rules, &identity)
}

/// Only what these decks can reach, from the same inputs and loader as the whole selection.
pub(crate) fn select_decks(decks: &[PlayerDeck; 2]) -> sve_engine::Result<SelectedEffects> {
    let (snapshot, keywords, input) = inputs();
    let (rules, identity) = rule_fixtures::resolved(snapshot, &bound_rules(snapshot))?;
    Game::select_player_decks(
        loader(),
        snapshot,
        keywords,
        input,
        Some((&rules, &identity)),
        decks,
    )
}

pub(crate) fn select_without_rules(
    snapshot: &str,
    keywords: &str,
    input: &Value,
) -> sve_engine::Result<SelectedEffects> {
    loader().select(snapshot, keywords, input)
}
