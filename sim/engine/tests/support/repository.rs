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
use sve_engine::catalog::{Catalog, EngineIdentityInput, SelectedEffects};
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
        let keywords = read_to_string(root.join("authored/rules/keywords.yaml")).unwrap();
        (snapshot, keywords, input)
    })
}

pub(crate) fn selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, input) = inputs();
        Catalog::from_current_selection(snapshot, keywords, input).unwrap()
    })
}

pub(crate) fn selection_with_rules() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (snapshot, keywords, input) = inputs();
        let rules_source = read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../authored/rules/engine/index.yaml"),
        )
        .unwrap();
        let mut rules_document: Value = serde_saphyr::from_str(&rules_source).unwrap();
        // Batch upgrades change snapshot bytes without changing these rule capabilities.
        *rules_document
            .pointer_mut("/input/snapshot_sha256")
            .unwrap() = Value::String(rule_fixtures::hash(snapshot.as_bytes()));
        let bound_rules = serde_json::to_string(&rules_document).unwrap();
        Catalog::from_current_selection_with_rules(
            snapshot,
            keywords,
            input,
            &bound_rules,
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap()
    })
}

pub(crate) fn lowerer() -> &'static ExpandedLowerer {
    static LOWERER: OnceLock<ExpandedLowerer> = OnceLock::new();
    LOWERER.get_or_init(|| ExpandedLowerer::new().unwrap())
}
