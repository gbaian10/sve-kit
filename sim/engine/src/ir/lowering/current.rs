//! Internal identity handoff from Python's complete current-source reader.

use super::{ExpandedLowerer, LoweringError, Nat, WellFormedCardProgram, error};
use alloc::collections::{BTreeMap, BTreeSet};
use serde::Deserialize;
use serde_json::Value;

/// Current source facts and their lowered candidate, without use qualification.
#[derive(Debug)]
pub struct CurrentCardProgram {
    /// Permanent owner selected by the current reader's JP printing lookup.
    pub card_id: String,
    /// Unmodified JP printing coordinate.
    pub card_no: String,
    /// Exact current-source fingerprint.
    pub source_version: String,
    /// Complete current faces and explicit token selection, retained separately from IR.
    pub source_payload: Value,
    /// Well-formed IR; this alone grants neither whole-card admission nor execution.
    pub program: WellFormedCardProgram,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentInput {
    tree: Value,
    card_id: String,
    card_no: String,
    source_version: String,
    source_payload: Value,
}

pub(super) struct CurrentIdentity {
    card_id: String,
    faces: BTreeMap<String, (String, Option<Nat>)>,
}

fn invalid(path: &str) -> LoweringError {
    error("invalid_current_identity", path)
}

fn permanent(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

impl CurrentIdentity {
    pub(super) fn printed_cost(
        &self,
        source: &Value,
        path: &str,
    ) -> Result<Option<Nat>, LoweringError> {
        let revision = source
            .pointer("/occurrence/owner/revision_id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(path))?;
        self.faces
            .get(revision)
            .map(|(_, cost)| *cost)
            .ok_or_else(|| invalid(path))
    }

    pub(super) fn owner(
        &self,
        source: &Value,
        path: &str,
    ) -> Result<(String, String), LoweringError> {
        let owner = source
            .pointer("/occurrence/owner")
            .ok_or_else(|| invalid(path))?;
        if owner.get("kind").and_then(Value::as_str) != Some("face_revision") {
            return Err(invalid(path));
        }
        let face = owner
            .get("revision_id")
            .and_then(Value::as_str)
            .and_then(|revision| self.faces.get(revision))
            .ok_or_else(|| invalid(path))?;
        Ok((self.card_id.clone(), face.0.clone()))
    }
}

pub(super) fn lower(
    compiler: &ExpandedLowerer,
    input: &Value,
) -> Result<CurrentCardProgram, LoweringError> {
    let input: CurrentInput =
        serde_json::from_value(input.clone()).map_err(|_private| invalid(""))?;
    let cards = input
        .tree
        .get("cards")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("/cards"))?;
    if !permanent(&input.card_id, "c:")
        || cards.len() != 1
        || !cards.contains_key(&input.card_no)
        || cards
            .get(&input.card_no)
            .and_then(|card| card.pointer("/meta/source"))
            .and_then(Value::as_str)
            != Some(input.source_version.as_str())
        || input.source_payload.get("recipe").and_then(Value::as_str) != Some("effect-source-v1")
        || input.source_payload.get("region").and_then(Value::as_str) != Some("jp")
    {
        return Err(invalid(""));
    }
    let tokens = input
        .source_payload
        .get("token_links")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("/source_payload/token_links"))?;
    if !tokens.is_empty() {
        return Err(error(
            "unsupported_token_selection",
            "/source_payload/token_links",
        ));
    }
    let faces = input
        .source_payload
        .get("faces")
        .and_then(Value::as_array)
        .filter(|faces| !faces.is_empty())
        .ok_or_else(|| invalid("/source_payload/faces"))?;
    let mut identity = CurrentIdentity {
        card_id: input.card_id.clone(),
        faces: BTreeMap::new(),
    };
    let mut face_ids = BTreeSet::new();

    for face in faces {
        let id = face
            .get("face_id")
            .and_then(Value::as_str)
            .filter(|id| permanent(id, "f:"))
            .ok_or_else(|| invalid("/source_payload/faces"))?;
        let revision = face
            .get("revision_id")
            .and_then(Value::as_str)
            .filter(|revision| !revision.is_empty())
            .ok_or_else(|| invalid("/source_payload/faces"))?;
        let cost = (face
            .pointer("/attributes/cost/state")
            .and_then(Value::as_str)
            == Some("known"))
        .then(|| {
            face.pointer("/attributes/cost/value")
                .and_then(Value::as_u64)
                .map(Nat)
        })
        .flatten();
        if !face_ids.insert(id)
            || identity
                .faces
                .insert(revision.into(), (id.into(), cost))
                .is_some()
        {
            return Err(invalid("/source_payload/faces"));
        }
    }
    let mut lowered = compiler.lower_coordinates(&input.tree, &BTreeMap::new(), Some(&identity))?;
    let program = lowered
        .remove(&input.card_no)
        .ok_or_else(|| invalid("/cards"))?;
    Ok(CurrentCardProgram {
        card_id: input.card_id,
        card_no: input.card_no,
        source_version: input.source_version,
        source_payload: input.source_payload,
        program,
    })
}
