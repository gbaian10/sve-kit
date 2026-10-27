//! Closed non-body obligations; compiler coordinates never become source scopes.

use super::{
    CurrentIdentity, CurrentInput, CurrentSourceEvidence, evidence_path, invalid_source, permanent,
};
use crate::ir::SourceSpan;
use crate::ir::lowering::{CardCoordinates, LoweringError, ast, error, source_span};
use alloc::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Non-body source evidence from the trusted current reader, without runtime admission.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum NonBodySource {
    /// Inert metadata and reminder evidence cannot contain an executable declaration.
    #[serde(rename = "metadata")]
    Metadata(MetadataObligation),
    /// An independently reconstructed, ordered printed-keyword obligation.
    #[serde(rename = "printed_keywords")]
    PrintedKeywords(PrintedKeywordsObligation),
}

/// Closed metadata evidence retained separately from executable declarations.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataObligation {
    binding_id: String,
    occurrence: Occurrence,
    semantic_key: MetadataKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
enum MetadataKey {
    #[serde(rename = "metadata.name.v1")]
    Name,
    #[serde(rename = "metadata.label.v1")]
    Label,
    #[serde(rename = "metadata.layout.v1")]
    Layout,
    #[serde(rename = "pure_reminder.v1")]
    Reminder,
}

/// A source reference has no Projection ability scope or body version.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardKeywordSourceUse {
    binding_id: String,
    occurrence: Occurrence,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Occurrence {
    owner: Owner,
    field: Field,
    #[serde(deserialize_with = "Option::deserialize")]
    ordinal: Option<u64>,
    source_hash: String,
    line_ordinal: u64,
    role: Role,
    segments: Vec<Segment>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Owner {
    #[serde(rename = "face_revision")]
    FaceRevision { revision_id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Field {
    Name,
    Effect,
    Section,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Body,
    Reminder,
    TokenHeader,
    Layout,
    Name,
    Label,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Segment {
    start: u64,
    end: u64,
}

/// Exact printed declaration payload, in source order.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KeywordDeclarationPayload {
    keywords: Vec<PrintedKeyword>,
    ports: EmptyPorts,
}

/// The current reader's closed printed-keyword subset; runtime support is separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintedKeyword {
    /// Ward declaration, without claiming its rules are executable.
    Ward,
    /// Aura declaration, without claiming its rules are executable.
    Aura,
    /// Printed Storm; runtime admission still requires the complete named shape.
    Storm,
    /// Printed Rush.
    Rush,
    /// Printed Bane.
    Bane,
    /// Printed Intimidate.
    Intimidate,
    /// Printed Assail.
    Assail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EmptyPorts {
    imports: Empty,
    exports: Empty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "The wire requires an empty object, never null."
)]
struct Empty {}

/// Independent C0 obligation; no author path or synthetic source scope.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedKeywordsObligation {
    source_use: CardKeywordSourceUse,
    face_id: String,
    frame_id: String,
    interface_key: String,
    semantic_key: KeywordSemantic,
    declaration: KeywordDeclarationPayload,
    source_adapter_recipe: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
enum KeywordSemantic {
    #[serde(rename = "card_keywords.v2")]
    CardKeywords,
}

/// Exact declaration evidence emitted only after C0 whole-card reconciliation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeclarationInput {
    #[serde(rename = "kind")]
    _kind: DeclarationKind,
    path: Vec<Value>,
    #[serde(rename = "source_proof")]
    _source_proof: CurrentProof,
    input_hash: String,
    source_input_hash: String,
    source_use: CardKeywordSourceUse,
    face_id: String,
    frame_id: String,
    interface_key: String,
    semantic_key: KeywordSemantic,
    declaration: KeywordDeclarationPayload,
    source_adapter_recipe: String,
    expansion_recipe: String,
    adapter_recipe: String,
}

#[derive(Clone, Copy, Debug, Deserialize)]
enum DeclarationKind {
    #[serde(rename = "printed_keywords")]
    PrintedKeywords,
}
#[derive(Clone, Copy, Debug, Deserialize)]
enum CurrentProof {
    #[serde(rename = "current_source_checked")]
    CurrentSourceChecked,
}

/// Checked declaration provenance. The scope in `source` is a compiler coordinate.
#[derive(Debug)]
pub struct CurrentKeywordEvidence {
    /// Permanent owner and original author location, with a local declaration scope.
    pub source: SourceSpan,
    /// Independent exact source obligation; it contains no compiler scope.
    pub reference: Option<PrintedKeywordsObligation>,
    /// Direct author declarations carry keywords without translation qualification.
    pub direct_keywords: Vec<PrintedKeyword>,
    /// Hash of original complete author-file bytes, not selected expanded-tree bytes.
    pub input_hash: String,
    /// Hash of the full current owner field, not just this line.
    pub source_input_hash: String,
    /// Exact expansion recipe checked against this reader's supported handoff.
    pub expansion_recipe: String,
    /// Exact author adapter recipe, distinct from the source adapter recipe.
    pub adapter_recipe: String,
}

impl CurrentKeywordEvidence {
    /// Ordered keywords retained without granting runtime support.
    #[must_use]
    pub fn keywords(&self) -> &[PrintedKeyword] {
        self.reference
            .as_ref()
            .map_or(&self.direct_keywords, |reference| {
                &reference.declaration.keywords
            })
    }
}

impl DeclarationInput {
    fn reference(&self) -> PrintedKeywordsObligation {
        PrintedKeywordsObligation {
            source_use: self.source_use.clone(),
            face_id: self.face_id.clone(),
            frame_id: self.frame_id.clone(),
            interface_key: self.interface_key.clone(),
            semantic_key: self.semantic_key,
            declaration: self.declaration.clone(),
            source_adapter_recipe: self.source_adapter_recipe.clone(),
        }
    }
}

fn closure(path: &str) -> LoweringError {
    error("current_keyword_closure_mismatch", path)
}

fn json<T>(value: &T, path: &str) -> Result<Value, LoweringError>
where
    T: Serialize,
{
    serde_json::to_value(value).map_err(|_private| invalid_source(path))
}

impl Occurrence {
    fn valid(&self) -> bool {
        let Owner::FaceRevision { revision_id } = &self.owner;
        let safe = 0x001f_ffff_ffff_ffff_u64;
        !revision_id.is_empty()
            && (self.field == Field::Section) == self.ordinal.is_some()
            && self.ordinal.is_none_or(|n| n <= safe)
            && self.line_ordinal <= safe
            && permanent(&self.source_hash, "", 64)
            && !self.segments.is_empty()
            && self
                .segments
                .iter()
                .all(|span| span.start < span.end && span.end <= safe)
            && self
                .segments
                .iter()
                .zip(self.segments.iter().skip(1))
                .all(|(left, right)| left.end <= right.start)
    }
}

impl PrintedKeywordsObligation {
    fn valid(&self) -> bool {
        permanent(&self.source_use.binding_id, "bind:", 64)
            && self.source_use.occurrence.valid()
            && self.source_use.occurrence.role == Role::Body
            && self.source_use.occurrence.field != Field::Name
            && permanent(&self.face_id, "f:", 32)
            && permanent(&self.frame_id, "frame:", 64)
            && permanent(&self.interface_key, "", 64)
            && !self.declaration.keywords.is_empty()
            && self.declaration.keywords.len() <= 7
            && self
                .declaration
                .keywords
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                == self.declaration.keywords.len()
    }
}

fn required<'input>(
    input: &'input CurrentInput,
    path: &str,
) -> Result<BTreeMap<String, &'input PrintedKeywordsObligation>, LoweringError> {
    let mut result = BTreeMap::new();
    let mut face_keywords = BTreeSet::new();
    for item in &input.source_inventory.non_body {
        match item {
            NonBodySource::Metadata(metadata) => {
                if metadata.binding_id.is_empty() || !metadata.occurrence.valid() {
                    return Err(invalid_source(path));
                }
            }
            NonBodySource::PrintedKeywords(obligation) => {
                if !obligation.valid() {
                    return Err(invalid_source(path));
                }
                let key = json(&obligation.source_use, path)?.to_string();
                if result.insert(key, obligation).is_some()
                    || obligation
                        .declaration
                        .keywords
                        .iter()
                        .any(|keyword| !face_keywords.insert((&obligation.face_id, keyword)))
                {
                    return Err(closure(path));
                }
            }
        }
    }
    Ok(result)
}

fn field_matches(
    input: &CurrentInput,
    occurrence: &Occurrence,
    path: &str,
) -> Result<bool, LoweringError> {
    let Owner::FaceRevision { revision_id } = &occurrence.owner;
    let field = json(&occurrence.field, path)?;
    let face = input
        .source_payload
        .get("faces")
        .and_then(Value::as_array)
        .and_then(|faces| {
            faces.iter().find(|face| {
                face.get("revision_id").and_then(Value::as_str) == Some(revision_id.as_str())
            })
        })
        .ok_or_else(|| invalid_source(path))?;
    let texts = face
        .get("texts")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_source(path))?;
    Ok(texts.iter().any(|text| {
        text.get("field") == Some(&field)
            && text.get("ordinal") == Some(&serde_json::json!(occurrence.ordinal))
            && text.get("state").and_then(Value::as_str) == Some("present")
            && text.get("source_hash").and_then(Value::as_str)
                == Some(occurrence.source_hash.as_str())
    }))
}

fn checked_one(
    input: &CurrentInput,
    identity: &CurrentIdentity,
    path: &str,
    ability: &ast::Ability,
    evidence: &DeclarationInput,
    input_hash: &str,
) -> Result<CurrentKeywordEvidence, LoweringError> {
    let source = ability.sources[0].json();
    let source_use: CardKeywordSourceUse =
        serde_json::from_value(source).map_err(|_private| invalid_source(path))?;
    let occurrence = &source_use.occurrence;
    let reference = evidence.reference();
    if !reference.valid()
        || source_use != evidence.source_use
        || !permanent(&evidence.input_hash, "sha256:", 64)
        || input_hash != evidence.input_hash
        || evidence.source_input_hash != format!("sha256:{}", occurrence.source_hash)
        || u64::from(ability.line).checked_sub(1) != Some(occurrence.line_ordinal)
        || ability.section != occurrence.ordinal
        || json(&reference.declaration, path)?
            != serde_json::json!({"keywords": ability.keywords, "ports": {"imports": {}, "exports": {}}})
        || !field_matches(input, occurrence, path)?
    {
        return Err(invalid_source(path));
    }
    let coordinates = CardCoordinates {
        key: &input.card_no,
        identity: Some(identity),
    };
    let span = source_span(&coordinates, path, ability)?;
    if span.face != evidence.face_id {
        return Err(invalid_source(path));
    }
    Ok(CurrentKeywordEvidence {
        source: span,
        reference: Some(reference),
        direct_keywords: Vec::new(),
        input_hash: evidence.input_hash.clone(),
        source_input_hash: evidence.source_input_hash.clone(),
        expansion_recipe: evidence.expansion_recipe.clone(),
        adapter_recipe: evidence.adapter_recipe.clone(),
    })
}

pub(super) fn checked(
    input: &CurrentInput,
    identity: &CurrentIdentity,
    entries: &[(String, ast::Ability)],
    body: &BTreeMap<String, CurrentSourceEvidence>,
) -> Result<BTreeMap<String, CurrentKeywordEvidence>, LoweringError> {
    let card_path = format!("/cards/{}", super::pointer_key(&input.card_no));
    let mut required = required(input, &card_path)?;
    let mut supplied = BTreeMap::new();
    for item in &input.declaration_sidecar {
        let path = evidence_path(&item.path)?;
        if supplied.insert(path.clone(), item).is_some() {
            return Err(invalid_source(&path));
        }
    }
    let keyword_entries: Vec<_> = entries
        .iter()
        .filter(|(_, ability)| ability.keywords.is_some())
        .collect();
    if supplied.len() != keyword_entries.len() || required.len() != keyword_entries.len() {
        return Err(closure(&card_path));
    }
    let mut input_hash = body
        .values()
        .next()
        .map(|evidence| evidence.input_hash.as_str());
    if body
        .values()
        .any(|evidence| Some(evidence.input_hash.as_str()) != input_hash)
    {
        return Err(invalid_source(&card_path));
    }
    let mut result = BTreeMap::new();
    for (path, ability) in keyword_entries {
        let evidence = supplied.remove(path).ok_or_else(|| invalid_source(path))?;
        // The complete original input bytes stay in C0; D0 checks their shared pin.
        let expected_hash = *input_hash.get_or_insert(&evidence.input_hash);
        let checked = checked_one(input, identity, path, ability, evidence, expected_hash)?;
        let reference = checked.reference.as_ref().ok_or_else(|| closure(path))?;
        let key = json(&reference.source_use, path)?.to_string();
        let expected = required.remove(&key).ok_or_else(|| closure(path))?;
        if reference != expected {
            return Err(closure(path));
        }
        result.insert(path.clone(), checked);
    }
    if !required.is_empty() || !supplied.is_empty() {
        return Err(closure(&card_path));
    }
    Ok(result)
}
