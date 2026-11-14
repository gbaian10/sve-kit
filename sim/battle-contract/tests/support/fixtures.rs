//! Fixture loading and systematic damage shared by the test binaries.

#![allow(
    dead_code,
    reason = "Each test binary uses its own part of these helpers."
)]
#![expect(
    clippy::unwrap_used,
    reason = "A missing or malformed fixture must fail the test."
)]

use serde_json::Value;
use std::fs;
use std::path::Path;

/// Every fixture of one group as (path, text), in a stable order.
pub(crate) fn fixtures(group: &str) -> Vec<(String, String)> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(group);
    let mut found: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let text = fs::read_to_string(&path).unwrap();
            (path.display().to_string(), text)
        })
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no fixture in {group}");
    found
}

fn object_pointers(value: &Value, pointer: &str, found: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            found.push(pointer.to_owned());
            for (key, field) in fields {
                object_pointers(field, &format!("{pointer}/{key}"), found);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                object_pointers(item, &format!("{pointer}/{index}"), found);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

/// Every copy of the message with one field removed or one unknown field added.
pub(crate) fn damaged(original: &Value) -> Vec<(String, Value)> {
    let mut pointers = Vec::new();
    object_pointers(original, "", &mut pointers);
    let mut copies = Vec::new();
    for pointer in pointers {
        let mut extended = original.clone();
        let fields = extended
            .pointer_mut(&pointer)
            .unwrap()
            .as_object_mut()
            .unwrap();
        let keys: Vec<String> = fields.keys().cloned().collect();
        fields.insert("unknown".to_owned(), Value::Null);
        copies.push((format!("unknown field in {pointer}"), extended));
        // The object table is keyed by handle: fewer entries are still a table.
        if pointer.ends_with("/objects") {
            continue;
        }
        for key in keys {
            let mut reduced = original.clone();
            reduced
                .pointer_mut(&pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(&key);
            copies.push((format!("missing {key} in {pointer}"), reduced));
        }
    }
    copies
}
