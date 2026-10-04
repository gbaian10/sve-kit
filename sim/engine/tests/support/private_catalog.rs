//! Shared private-snapshot gate for the two snapshot-dependent test targets.

use alloc::sync::Arc;
use std::env::var_os;
use std::path::PathBuf;
use std::sync::OnceLock;

use sve_engine::catalog::Catalog;

#[expect(
    clippy::expect_used,
    clippy::print_stderr,
    reason = "Explicit inputs fail closed; optional local skips need a visible notice."
)]
pub(crate) fn catalog() -> Option<Arc<Catalog>> {
    static CATALOG: OnceLock<Option<Arc<Catalog>>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let full = var_os("SVE_CI_TEST_MODE").as_deref().map_or_else(
            || var_os("CI").is_some(),
            |mode| {
                assert!(mode == "full" || mode == "fork", "unknown Rust test scope");
                mode == "full"
            },
        );
        let required = var_os("SVE_PRIVATE_TESTDATA_MODE").as_deref().is_some_and(|mode| {
            assert!(mode == "required" || mode == "excluded", "unknown private test-data mode");
            mode == "required"
        });
        let Some(snapshot) = var_os("SVE_TEST_SNAPSHOT") else {
            assert!(!(full || required), "required Rust tests need SVE_TEST_SNAPSHOT");
            eprintln!("SKIP: private snapshot tests were not run; set SVE_TEST_SNAPSHOT to enable them.");
            return None;
        };
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        Some(Arc::new(
            Catalog::load(&PathBuf::from(snapshot), &root.join("authored"))
                .expect("validated private card snapshot"),
        ))
    }).as_ref().map(Arc::clone)
}
