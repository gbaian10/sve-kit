//! Historical regression catalogue from carddb's temporary public-input projection.
//!
//! Cards migrated to author 1.0 leave the prototype set, but the shared scenarios still
//! pin their original behaviour. This catalogue restores each migrated card's frozen
//! prototype AST for those scenarios only; it is never a current-selection result and
//! grants no runtime qualification (see `current_repository.rs` for the real selection).

use alloc::sync::Arc;
use std::env::var_os;
use std::ffi::OsStr;
use std::fs::{read_dir, read_to_string};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::Value;
use sve_engine::catalog::{Catalog, EngineIdentityInput};

#[expect(
    clippy::expect_used,
    reason = "Missing generated input must fail every test scope."
)]
fn restore_frozen_programs(documents: &mut [(String, String)], root: &Path) {
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
}

#[expect(
    clippy::expect_used,
    reason = "Missing generated input must fail every test scope."
)]
pub(crate) fn historical_regression_catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let snapshot = var_os("SVE_TEST_SNAPSHOT")
            .expect("generate test input first: python3 .github/ci/rust_tests.py");
        let selection = var_os("SVE_TEST_CURRENT_SELECTION")
            .expect("generate current selection first: python3 .github/ci/rust_tests.py");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let input: Value =
            serde_json::from_str(&read_to_string(selection).expect("current selection"))
                .expect("current selection JSON");
        let snapshot = read_to_string(snapshot).expect("public card test input");
        let keywords =
            read_to_string(root.join("authored/rules/keywords.yaml")).expect("keyword registry");
        // The real selection must still be accepted before any historical program is restored.
        Catalog::from_current_selection(&snapshot, &keywords, &input)
            .expect("complete generated public current selection");
        let mut documents: Vec<(String, String)> = input
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
        restore_frozen_programs(&mut documents, &root);
        Arc::new(
            Catalog::from_documents_with_rules(
                &snapshot,
                &keywords,
                &documents,
                &read_to_string(root.join("authored/rules/engine/index.yaml"))
                    .expect("rule bindings"),
                &EngineIdentityInput::LegacyJp,
            )
            .expect("selected prototypes with frozen migrated programs and rule bindings"),
        )
    }))
}
