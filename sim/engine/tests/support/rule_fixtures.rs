//! Small synthetic inputs shared by the rule-binding integration targets.

#![allow(
    dead_code,
    reason = "Integration binaries exercise different portions of this shared fixture."
)]
#![expect(
    clippy::indexing_slicing,
    reason = "Fixture objects have their complete literal shape."
)]

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

pub(crate) fn hash(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        result.push(char::from(HEX[usize::from(byte >> 4_u8)]));
        result.push(char::from(HEX[usize::from(byte & 15)]));
    }
    result
}

pub(crate) fn face(number: &str) -> Value {
    json!({"kind":"legacy-face","region":"jp","card_no":number,"face_ordinal":0_u8})
}

pub(crate) fn title(code: &str, number: &str, capabilities: Value) -> Value {
    let mut result = json!({"title_code":code,"anchor":face(number),"evidence":["synthetic"]});
    result["capabilities"] = capabilities;
    result
}

pub(crate) fn resource(role: &str, number: &str, source: &str, template: bool) -> Value {
    let mut result = json!({"role":role,"names":[{"kind":"legacy-name","face":face(number),"name_source":source}],"evidence":["synthetic"]});
    if template {
        result["template"] = face(number);
    }
    result
}

pub(crate) fn rules(snapshot: &str, titles: Value, resources: Value) -> Value {
    let mut result = json!({"version":"engine-rules/1","scope":{"region":"jp","rules_version":"synthetic-1","rules_source_version_id":format!("src:v1:{}", "a".repeat(64))},
        "input":{"kind":"legacy-jp","snapshot_sha256":hash(snapshot.as_bytes())},
        "evidence":[{"id":"synthetic","source_version_id":format!("src:v1:{}", "a".repeat(64)),"rule_refs":["1.2.3"],"checked_on":"2026-10-03","checked_by":"synthetic-model"}]});
    result["titles"] = titles;
    result["resources"] = resources;
    result
}
