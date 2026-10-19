//! Shared catalogue from carddb's temporary public-input projection.

use alloc::sync::Arc;
use std::env::var_os;
use std::path::PathBuf;
use std::sync::OnceLock;

use sve_engine::catalog::Catalog;

#[expect(
    clippy::expect_used,
    reason = "Missing generated input must fail every test scope."
)]
pub(crate) fn catalog() -> Arc<Catalog> {
    static CATALOG: OnceLock<Arc<Catalog>> = OnceLock::new();
    Arc::clone(CATALOG.get_or_init(|| {
        let snapshot = var_os("SVE_TEST_SNAPSHOT")
            .expect("generate test input first: python3 .github/ci/rust_tests.py");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        Arc::new(
            Catalog::load(&PathBuf::from(snapshot), &root.join("authored"))
                .expect("generated public card test input"),
        )
    }))
}
