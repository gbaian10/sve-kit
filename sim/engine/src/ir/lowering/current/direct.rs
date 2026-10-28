//! Direct author admission retains identity and whole-card coverage without source equivalence.

use super::{CurrentCardProgram, CurrentIdentity, CurrentKeywordEvidence, CurrentSourceEvidence};
use crate::ir::lowering::{
    CardCoordinates, ExpandedLowerer, LoweringError, ability_entries, ast, decode, error,
    pointer_key,
};
use crate::ir::{Nat, SourceSpan};
use alloc::collections::{BTreeMap, BTreeSet};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    admission: Admission,
    tree: Value,
    card_id: String,
    card_no: String,
    source_payload: Payload,
}

#[derive(Deserialize)]
enum Admission {
    #[serde(rename = "direct")]
    Direct,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    card_id: String,
    faces: Vec<PrintedFace>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrintedFace {
    face_id: String,
    revision_id: String,
    ordinal: usize,
    #[serde(rename = "side")]
    _side: String,
    cost: Value,
    fields: Vec<FieldLines>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FieldLines {
    field: String,
    ordinal: Option<u64>,
    lines: Vec<u32>,
    section_kind: Option<String>,
}

fn invalid(path: &str) -> LoweringError {
    error("invalid_direct_admission", path)
}

pub(in crate::ir::lowering) fn source_span(
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    source: &ast::DirectSourceUse,
) -> Result<SourceSpan, LoweringError> {
    let identity = key.identity.ok_or_else(|| invalid(path))?;
    let revision = source
        .owner
        .get("revision_id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(path))?;
    let (face, _) = identity.faces.get(revision).ok_or_else(|| invalid(path))?;
    if source.owner.get("kind").and_then(Value::as_str) != Some("face_revision")
        || face != &source.face_id
        || source.line != entry.line
        || source.ordinal != entry.section
        || !matches!(source.admission.as_str(), "light" | "shared")
        || !path.starts_with(&format!(
            "/cards/{}/faces/{}/abilities/",
            pointer_key(key.key),
            pointer_key(face)
        ))
    {
        return Err(invalid(path));
    }
    Ok(SourceSpan {
        card_id: identity.card_id.clone(),
        card_no: key.key.to_owned(),
        face: face.clone(),
        owner: source.owner.to_string(),
        field: source.field.clone(),
        section: source.ordinal.map(|n| n.to_string()),
        line: source.line,
        scope: if entry.keywords.is_some() {
            format!("keyword_declaration:{path}")
        } else {
            source.scope.clone()
        },
        fragment: None,
        author_path: path.to_owned(),
    })
}

type Line = (String, Option<u64>, u32);

fn coverage(
    source: &ast::DirectSourceUse,
    expected: &BTreeSet<Line>,
    covered: &mut BTreeSet<Line>,
) -> Result<(), LoweringError> {
    if source.line == 0
        || !source.covers.contains(&source.line)
        || source.covers.is_empty()
        || source.covers.iter().collect::<BTreeSet<_>>().len() != source.covers.len()
        || (source.field == "section") != source.ordinal.is_some()
        || !matches!(source.field.as_str(), "effect" | "section")
    {
        return Err(invalid("/sources"));
    }
    for line in &source.covers {
        let key = (source.face_id.clone(), source.ordinal, *line);
        if !expected.contains(&key) {
            return Err(invalid("/sources/covers"));
        }
        covered.insert(key);
    }
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "Reconcile the complete line inventory before constructing an executable candidate."
)]
pub(super) fn lower(
    compiler: &ExpandedLowerer,
    value: &Value,
) -> Result<CurrentCardProgram, LoweringError> {
    let input: Input = decode(value.clone(), "")?;
    let Admission::Direct = input.admission;
    compiler
        .validator
        .validate(&input.tree)
        .map_err(|failure| error("invalid_expanded", &failure.instance_path().to_string()))?;
    if input.card_id != input.source_payload.card_id
        || !super::permanent(&input.card_id, "c:", 32)
        || input.source_payload.faces.is_empty()
    {
        return Err(invalid("/card_id"));
    }
    let mut identity = CurrentIdentity {
        card_id: input.card_id.clone(),
        faces: BTreeMap::new(),
    };
    let mut ordinals = BTreeMap::new();
    let mut expected = BTreeSet::new();
    for face in &input.source_payload.faces {
        let cost = face
            .cost
            .as_u64()
            .or_else(|| face.cost.as_str()?.parse().ok())
            .map(Nat);
        if !super::permanent(&face.face_id, "f:", 32)
            || face.revision_id.is_empty()
            || identity
                .faces
                .insert(face.revision_id.clone(), (face.face_id.clone(), cost))
                .is_some()
            || ordinals
                .insert(face.face_id.clone(), face.ordinal)
                .is_some()
            || !face
                .fields
                .iter()
                .any(|field| field.field == "effect" && field.ordinal.is_none())
        {
            return Err(invalid("/source_payload/faces"));
        }
        let mut fields = BTreeSet::new();
        for field in &face.fields {
            if !fields.insert((field.field.as_str(), field.ordinal))
                || (field.field == "section") != field.ordinal.is_some()
                || !matches!(field.field.as_str(), "effect" | "section")
            {
                return Err(invalid("/source_payload/fields"));
            }
            for line in &field.lines {
                if *line == 0 || !expected.insert((face.face_id.clone(), field.ordinal, *line)) {
                    return Err(invalid("/source_payload/lines"));
                }
            }
        }
    }
    if ordinals.values().copied().collect::<BTreeSet<_>>() != (0..ordinals.len()).collect() {
        return Err(invalid("/source_payload/faces"));
    }
    let cards = input
        .tree
        .get("cards")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("/cards"))?;
    if cards.len() != 1 {
        return Err(invalid("/cards"));
    }
    let card_path = format!("/cards/{}", pointer_key(&input.card_no));
    let card: ast::Card = decode(
        cards
            .get(&input.card_no)
            .cloned()
            .ok_or_else(|| invalid(&card_path))?,
        &card_path,
    )?;
    let ignored = card.ignored.clone();
    let tokens = card.token_sources.clone();
    let token_lines = input
        .source_payload
        .faces
        .iter()
        .flat_map(|face| {
            face.fields
                .iter()
                .filter(|field| field.section_kind.as_deref() == Some("token_definition"))
                .flat_map(|field| {
                    field
                        .lines
                        .iter()
                        .map(|line| (face.face_id.clone(), field.ordinal, *line))
                })
        })
        .collect::<BTreeSet<_>>();
    let entries = ability_entries(card, &card_path)?;
    let coordinates = CardCoordinates {
        key: &input.card_no,
        identity: Some(&identity),
    };
    let mut covered = BTreeSet::new();
    let mut uses = BTreeSet::new();
    let mut source_sidecar = BTreeMap::new();
    let mut declarations = BTreeMap::new();
    for (path, ability) in &entries {
        let ast::SourceUse::Direct(source) = &ability.sources[0] else {
            return Err(invalid(path));
        };
        coverage(source, &expected, &mut covered)?;
        if source
            .covers
            .iter()
            .any(|line| token_lines.contains(&(source.face_id.clone(), source.ordinal, *line)))
        {
            return Err(invalid(path));
        }
        if !uses.insert((
            source.face_id.clone(),
            source.ordinal,
            source.line,
            source.scope.clone(),
        )) {
            return Err(invalid(path));
        }
        let source_span = source_span(&coordinates, path, ability, source)?;
        if let Some(keywords) = &ability.keywords {
            let keywords = keywords
                .iter()
                .map(|keyword| decode(Value::String(keyword.clone()), path))
                .collect::<Result<Vec<_>, _>>()?;
            declarations.insert(
                path.clone(),
                CurrentKeywordEvidence {
                    source: source_span,
                    reference: None,
                    direct_keywords: keywords,
                    input_hash: String::new(),
                    source_input_hash: String::new(),
                    expansion_recipe: String::new(),
                    adapter_recipe: String::new(),
                },
            );
        } else {
            source_sidecar.insert(
                path.clone(),
                CurrentSourceEvidence {
                    source: source_span,
                    source_use: ability.sources[0].json(),
                    input_hash: String::new(),
                    source_input_hash: String::new(),
                },
            );
        }
    }
    for source in &ignored {
        let revision = source
            .owner
            .get("revision_id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("/ignored"))?;
        if identity
            .faces
            .get(revision)
            .is_none_or(|(face, _)| face != &source.face_id)
            || source.covers.iter().any(|line| {
                covered.contains(&(source.face_id.clone(), source.ordinal, *line))
                    || token_lines.contains(&(source.face_id.clone(), source.ordinal, *line))
            })
        {
            return Err(invalid("/ignored"));
        }
        coverage(source, &expected, &mut covered)?;
    }
    for token in &tokens {
        let source = &token.source;
        let revision = source
            .owner
            .get("revision_id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("/token_sources"))?;
        let valid = input.source_payload.faces.iter().any(|face| {
            face.face_id == source.face_id
                && face.revision_id == revision
                && face.fields.iter().any(|field| {
                    field.ordinal == source.ordinal
                        && matches!(
                            field.section_kind.as_deref(),
                            Some("unknown" | "token_definition")
                        )
                })
        });
        if !valid
            || source.field != "section"
            || source.owner.get("kind").and_then(Value::as_str) != Some("face_revision")
            || source
                .covers
                .iter()
                .any(|line| covered.contains(&(source.face_id.clone(), source.ordinal, *line)))
        {
            return Err(invalid("/token_sources"));
        }
        coverage(source, &expected, &mut covered)?;
    }
    if covered != expected {
        return Err(error("current_line_closure_mismatch", &card_path));
    }
    let mut programs =
        compiler.lower_coordinates(&input.tree, &BTreeMap::new(), Some(&identity))?;
    Ok(CurrentCardProgram {
        card_id: input.card_id,
        card_no: input.card_no.clone(),
        source_version: "direct".into(),
        source_payload: value["source_payload"].clone(),
        expanded: input.tree,
        face_ordinals: ordinals,
        source_sidecar,
        declaration_sidecar: declarations,
        non_body_sources: Vec::new(),
        program: programs
            .remove(&input.card_no)
            .ok_or_else(|| invalid("/cards"))?,
    })
}
