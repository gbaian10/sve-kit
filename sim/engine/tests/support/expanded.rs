//! Real C0 expansion shared by candidate golden tests.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

pub(crate) fn tree() -> &'static Value {
    static TREE: OnceLock<Value> = OnceLock::new();
    TREE.get_or_init(|| expand(false))
}

pub(crate) fn spell_tree() -> &'static Value {
    static TREE: OnceLock<Value> = OnceLock::new();
    TREE.get_or_init(|| expand(true))
}

#[allow(
    clippy::expect_used,
    reason = "Missing C0 or invalid fixture input must fail tests."
)]
fn expand(spell: bool) -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut command = Command::new("uv");
    command
        .arg("run")
        .arg("--directory")
        .arg(root.join("carddb"))
        .args([
            "--locked",
            "--no-default-groups",
            "python",
            "-c",
            "
import json
import sys
from pathlib import Path
from sve_carddb.domains.effects.author_expansion import AuthorExpander
expander = AuthorExpander(Path(sys.argv[1]))
first = expander.expand(Path(sys.argv[2]).read_bytes(), []).tree
first['cards'] = {'BP08-038': first['cards']['BP08-038']}
for path in sys.argv[3:]:
    first['cards'].update(expander.expand(Path(path).read_bytes(), []).tree['cards'])
print(json.dumps(first))
",
        ])
        .arg(root.join("dsl"))
        .arg(root.join("carddb/tests/fixtures/author_entries/golden.yaml"))
        .arg(root.join("sim/engine/tests/fixtures/lowering/bp01-118.yaml"))
        .arg(root.join("sim/engine/tests/fixtures/lowering/numeric.yaml"))
        .arg(root.join("sim/engine/tests/fixtures/lowering/search.yaml"))
        .arg(root.join("sim/engine/tests/fixtures/lowering/activated.yaml"));
    if spell {
        command.arg(root.join("sim/engine/tests/fixtures/lowering/spell.yaml"));
    }
    let output = command.output().expect("run C0 fixture expansion");
    assert!(
        output.status.success(),
        "C0 expansion failed: {}",
        output.status
    );
    serde_json::from_slice(&output.stdout).expect("C0 expanded JSON")
}
