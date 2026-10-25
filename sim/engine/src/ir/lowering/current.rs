//! Internal identity handoff from Python's complete current-source reader.

use super::{
    CardCoordinates, ExpandedLowerer, LoweringError, Nat, SourceSpan, WellFormedCardProgram,
    ability_entries, decode, error, pointer_key, source_span,
};
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
    /// Expanded author tree; provenance stays in the separate source sidecar.
    pub expanded: Value,
    /// One current-checked source per ability, keyed by its author JSON pointer.
    pub source_sidecar: BTreeMap<String, CurrentSourceEvidence>,
    /// Source declarations outside ability bodies; retained without claiming execution.
    pub non_body_sources: Vec<Value>,
    /// Well-formed IR; this alone grants neither whole-card admission nor execution.
    pub program: WellFormedCardProgram,
}

/// Current-source proof retained at the internal C0/D0 boundary, not qualification.
#[derive(Debug)]
pub struct CurrentSourceEvidence {
    /// Permanent CardId/FaceId, revision owner and original author coordinates.
    pub source: SourceSpan,
    /// Exact checked `SourceUse`, including all occurrence segments.
    pub source_use: Value,
    /// Hash of the original author input bytes.
    pub input_hash: String,
    /// Hash of the current source text checked by C0.
    pub source_input_hash: String,
}

impl CurrentCardProgram {
    /// Resolve an expanded node or IR instruction's author path to its checked use.
    #[must_use]
    pub fn source_at(&self, mut author_path: &str) -> Option<&CurrentSourceEvidence> {
        loop {
            if let Some(evidence) = self.source_sidecar.get(author_path) {
                return Some(evidence);
            }
            (author_path, _) = author_path.rsplit_once('/')?;
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentInput {
    tree: Value,
    card_id: String,
    card_no: String,
    source_version: String,
    source_payload: Value,
    source_inventory: SourceInventory,
    sidecar: Vec<SourceEvidenceInput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceInventory {
    abilities: Vec<RequiredAbility>,
    non_body: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequiredAbility {
    source_use: Value,
    body: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceEvidenceInput {
    path: Vec<Value>,
    source_proof: String,
    source_use: Value,
    input_hash: String,
    source_input_hash: String,
}

fn invalid_source(path: &str) -> LoweringError {
    error("invalid_current_source", path)
}

fn evidence_path(parts: &[Value]) -> Result<String, LoweringError> {
    let mut path = String::new();
    for part in parts {
        path.push('/');
        match part {
            Value::String(key) => path.push_str(&pointer_key(key)),
            Value::Number(index) if index.is_u64() => path.push_str(&index.to_string()),
            Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::Array(_)
            | Value::Object(_) => return Err(invalid_source(&path)),
        }
    }
    Ok(path)
}

fn checked_sources(
    input: &CurrentInput,
    identity: &CurrentIdentity,
) -> Result<BTreeMap<String, CurrentSourceEvidence>, LoweringError> {
    let card_path = format!("/cards/{}", pointer_key(&input.card_no));
    let card = input
        .tree
        .pointer(&card_path)
        .ok_or_else(|| invalid(&card_path))?;
    let entries = ability_entries(decode(card.clone(), &card_path)?, &card_path)?;
    let mut supplied = BTreeMap::new();
    for item in &input.sidecar {
        let path = evidence_path(&item.path)?;
        if supplied.insert(path.clone(), item).is_some() {
            return Err(invalid_source(&path));
        }
    }
    if supplied.len() != entries.len() {
        return Err(invalid_source(&card_path));
    }
    let mut required = BTreeMap::new();
    for ability in &input.source_inventory.abilities {
        let key = source_key(&ability.source_use, &card_path)?;
        if required.insert(key, ability).is_some() {
            return Err(closure_error(&card_path));
        }
    }
    if required.len() != entries.len() {
        return Err(closure_error(&card_path));
    }
    let coordinates = CardCoordinates {
        key: &input.card_no,
        identity: Some(identity),
    };
    let mut result = BTreeMap::new();
    for (path, ability) in entries {
        if ability.keywords.is_some() {
            return Err(error("unsupported_keyword_source_mapping", &path));
        }
        let evidence = supplied.get(&path).ok_or_else(|| invalid_source(&path))?;
        let source_value = ability.sources[0].json();
        let source_use = &source_value;
        // `line == 0` must fail here: `None == None` below would otherwise pass it.
        let line_ordinal = source_use
            .pointer("/occurrence/line_ordinal")
            .and_then(Value::as_u64);
        let source_hash = source_use
            .pointer("/occurrence/source_hash")
            .and_then(Value::as_str);
        let expected_line = u64::from(ability.line).checked_sub(1);
        if expected_line.is_none()
            || evidence.source_proof != "current_source_checked"
            || evidence.source_use != *source_use
            || !permanent(&evidence.input_hash, "sha256:", 64)
            || !permanent(&evidence.source_input_hash, "sha256:", 64)
            || source_use.pointer("/occurrence/ordinal")
                != Some(&serde_json::json!(ability.section))
            || line_ordinal != expected_line
            || source_hash.map(|hash| format!("sha256:{hash}")).as_deref()
                != Some(evidence.source_input_hash.as_str())
        {
            return Err(invalid_source(&path));
        }
        let obligation = required
            .remove(&source_key(source_use, &path)?)
            .ok_or_else(|| closure_error(&path))?;
        let mut body = input
            .tree
            .pointer(&path)
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| invalid_source(&path))?;
        for key in ["line", "section", "sources"] {
            body.remove(key);
        }
        if obligation.source_use != *source_use || obligation.body != Value::Object(body) {
            return Err(closure_error(&path));
        }
        result.insert(
            path.clone(),
            CurrentSourceEvidence {
                source: source_span(&coordinates, &path, &ability)?,
                source_use: evidence.source_use.clone(),
                input_hash: evidence.input_hash.clone(),
                source_input_hash: evidence.source_input_hash.clone(),
            },
        );
    }
    Ok(result)
}

fn source_key(source: &Value, path: &str) -> Result<(String, String), LoweringError> {
    let binding = source.get("binding_id").and_then(Value::as_str);
    let scope = source.get("ability_scope").and_then(Value::as_str);
    binding
        .zip(scope)
        .map(|(binding, scope)| (binding.to_owned(), scope.to_owned()))
        .ok_or_else(|| closure_error(path))
}

fn closure_error(path: &str) -> LoweringError {
    error("current_ability_closure_mismatch", path)
}

pub(super) struct CurrentIdentity {
    card_id: String,
    faces: BTreeMap<String, (String, Option<Nat>)>,
}

fn invalid(path: &str) -> LoweringError {
    error("invalid_current_identity", path)
}

fn permanent(value: &str, prefix: &str, length: usize) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == length
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
    if !permanent(&input.card_id, "c:", 32)
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
            .filter(|id| permanent(id, "f:", 32))
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
    let source_sidecar = checked_sources(&input, &identity)?;
    let mut lowered = compiler.lower_coordinates(&input.tree, &BTreeMap::new(), Some(&identity))?;
    let program = lowered
        .remove(&input.card_no)
        .ok_or_else(|| invalid("/cards"))?;
    Ok(CurrentCardProgram {
        card_id: input.card_id,
        card_no: input.card_no,
        source_version: input.source_version,
        source_payload: input.source_payload,
        expanded: input.tree,
        source_sidecar,
        non_body_sources: input.source_inventory.non_body,
        program,
    })
}
