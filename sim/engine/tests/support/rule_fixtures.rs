//! Small synthetic inputs shared by the rule-binding integration targets.

#![allow(
    dead_code,
    reason = "Integration binaries exercise different portions of this shared fixture."
)]
#![expect(
    clippy::indexing_slicing,
    reason = "Fixture objects have their complete literal shape."
)]

extern crate alloc;

use alloc::collections::BTreeMap;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use sve_engine::catalog::{Catalog, EngineIdentityInput, FaceIdentity, SelectedEffects};
use sve_engine::{EngineFailure, Result};

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
    let mut result = json!({"format":1_u8,"kind":"engine_rules","scope":{"region":"jp","rules_version":"synthetic-1","rules_source_version_id":format!("src:v1:{}", "a".repeat(64))},
        "input":{"kind":"legacy-jp","snapshot_sha256":hash(snapshot.as_bytes())},
        "evidence":[{"id":"synthetic","source_version_id":format!("src:v1:{}", "a".repeat(64)),"rule_refs":["1.2.3"],"checked_on":"2026-10-03"}]});
    result["titles"] = titles;
    result["resources"] = resources;
    result
}

/// Give test locators stable identities before they enter the same boundary as published rules.
pub(crate) fn resolved(snapshot: &str, document: &str) -> Result<(String, EngineIdentityInput)> {
    let invalid_locator = || EngineFailure::Invalid("invalid rule fixture locator".into());
    let mut rules: Value = serde_saphyr::from_str(document)
        .map_err(|error| EngineFailure::Invalid(error.to_string()))?;
    if rules["input"]["kind"] != "legacy-jp" || rules["scope"]["region"] != "jp" {
        return Err(invalid_locator());
    }
    let mut faces = BTreeMap::new();
    let mut face = |locator: &Value, title: Option<String>| {
        if locator["kind"] != "legacy-face" || locator["region"] != "jp" {
            return Err(invalid_locator());
        }
        let number = locator["card_no"].as_str().ok_or_else(invalid_locator)?;
        let ordinal = locator["face_ordinal"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(invalid_locator)?;
        let id = format!("face:{number}:{ordinal}");
        let record = faces.entry(id.clone()).or_insert_with(|| FaceIdentity {
            card_no: number.into(),
            face_ordinal: ordinal,
            face_id: id,
            rules_name_id: format!("rules-name:{number}:{ordinal}"),
            title_code: None,
        });
        if title.is_some() {
            record.title_code = title;
        }
        Ok(record.face_id.clone())
    };

    for entry in rules["titles"].as_array_mut().ok_or_else(invalid_locator)? {
        let code = entry["title_code"]
            .as_str()
            .ok_or_else(invalid_locator)?
            .to_owned();
        let id = face(&entry["anchor"], Some(code))?;
        entry["anchor"] = json!({"kind":"face-id","region":"jp","face_id":id});
    }
    for entry in rules["resources"]
        .as_array_mut()
        .ok_or_else(invalid_locator)?
    {
        for name in entry["names"].as_array_mut().ok_or_else(invalid_locator)? {
            if name["kind"] != "legacy-name" {
                return Err(invalid_locator());
            }
            let id = face(&name["face"], None)?;
            name["face"] = json!({"kind":"face-id","region":"jp","face_id":id});
        }
        if let Some(template) = entry.get_mut("template") {
            let id = face(template, None)?;
            *template = json!({"kind":"face-id","region":"jp","face_id":id});
        }
    }
    rules["input"]["kind"] = json!("resolved");
    let identity = EngineIdentityInput::Resolved {
        region: "jp".into(),
        snapshot_sha256: hash(snapshot.as_bytes()),
        faces: faces.into_values().collect(),
    };
    Ok((rules.to_string(), identity))
}

pub(crate) fn catalog(
    snapshot: &str,
    keywords: &str,
    documents: &[(String, String)],
    rules: &str,
) -> Result<Catalog> {
    let (rules, identity) = resolved(snapshot, rules)?;
    Catalog::from_fixture_documents_with_rules(snapshot, keywords, documents, &rules, &identity)
}

pub(crate) fn selection(
    snapshot: &str,
    keywords: &str,
    input: &Value,
    rules: &str,
) -> Result<SelectedEffects> {
    let (rules, identity) = resolved(snapshot, rules)?;
    Catalog::from_current_selection_with_rules(snapshot, keywords, input, &rules, &identity)
}
