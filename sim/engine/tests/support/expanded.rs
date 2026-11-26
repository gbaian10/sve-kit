//! Load expanded test fixtures for the lowering and runtime tests.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

pub(crate) fn tree() -> &'static Value {
    static TREE: OnceLock<Value> = OnceLock::new();
    TREE.get_or_init(batch)
}

pub(crate) fn spell_tree() -> &'static Value {
    static SPELL: OnceLock<Value> = OnceLock::new();
    SPELL.get_or_init(|| {
        let mut tree = tree().clone();
        let mut values = load(&["sim/engine/tests/fixtures/lowering/spell.yaml"]);
        append_cards(&mut tree, values.pop().unwrap());
        tree
    })
}

fn batch() -> Value {
    let mut trees = load(&[
        "carddb/tests/fixtures/author_entries/golden.yaml",
        "sim/engine/tests/fixtures/lowering/bp01-118.yaml",
        "sim/engine/tests/fixtures/lowering/numeric.yaml",
        "sim/engine/tests/fixtures/lowering/search.yaml",
        "sim/engine/tests/fixtures/lowering/activated.yaml",
        "sim/engine/tests/fixtures/lowering/discard-all.yaml",
        "sim/engine/tests/fixtures/lowering/look-order.yaml",
    ])
    .into_iter();
    let mut first = trees.next().unwrap();
    first["cards"]
        .as_object_mut()
        .unwrap()
        .retain(|number, _| number == "BP08-038");
    for tree in trees {
        append_cards(&mut first, tree);
    }
    first
}

fn append_cards(batch: &mut Value, mut tree: Value) {
    let cards = tree["cards"].as_object_mut().unwrap();
    batch["cards"].as_object_mut().unwrap().append(cards);
}

#[allow(
    clippy::expect_used,
    reason = "Missing or invalid fixture input must fail tests."
)]
pub(crate) fn load(paths: &[&str]) -> Vec<Value> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("uv")
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
from sve_carddb.core.yaml import parse_yaml
print(json.dumps([parse_yaml(Path(p).read_bytes()) for p in sys.argv[1:]]))
",
        ])
        .args(paths.iter().map(|path| root.join(path)))
        .output()
        .expect("read expanded YAML fixture");
    assert!(
        output.status.success(),
        "fixture YAML loading failed: {}",
        output.status
    );
    serde_json::from_slice(&output.stdout).expect("expanded fixture JSON")
}
