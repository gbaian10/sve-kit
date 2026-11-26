//! Bind executable nodes to the adopted faces of the verified snapshot.

use super::{CurrentCardProgram, CurrentIdentity, CurrentKeywordEvidence, Nat, SourceSpan};
use crate::ir::lowering::{
    CardCoordinates, CardDeckRule, LoweringError, ast, decode, decode_abilities, error,
    lower_entries, pointer_key,
};
use alloc::collections::BTreeMap;
use core::mem::take;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Rules facts for an adopted face, ordered by permanent face ordinal.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramFace {
    /// Permanent face identifier.
    pub face_id: String,
    /// Adopted wording revision from the same region and manifest.
    pub revision_id: String,
    /// Printed play cost, when the face has one.
    pub cost: Option<u64>,
    /// Effective construction name, when the catalogue specifies it.
    pub rules_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Program {
    card_id: String,
    scope: String,
    #[serde(default)]
    abilities: Vec<Value>,
    #[serde(default)]
    faces: BTreeMap<String, ast::Face>,
    deck_rule: Option<CardDeckRule>,
    #[serde(rename = "token")]
    _token: Option<IgnoredAny>,
}

fn invalid(path: &str) -> LoweringError {
    error("invalid_published_program", path)
}

fn bind_abilities(
    abilities: &mut [Value],
    face: &ProgramFace,
    positions: &mut BTreeMap<(Option<u64>, u64), usize>,
    path: &str,
) -> Result<(), LoweringError> {
    for ability in abilities {
        let node = ability.as_object_mut().ok_or_else(|| invalid(path))?;
        let line = node
            .get("line")
            .and_then(Value::as_u64)
            .filter(|line| *line > 0)
            .ok_or_else(|| invalid(path))?;
        let section = node.get("section").and_then(Value::as_u64);
        let position = positions.entry((section, line)).or_default();
        let source = json!({
            "owner": {"kind": "face_revision", "revision_id": face.revision_id},
            "face_id": face.face_id,
            "scope": format!("ability_{position}"),
        });
        *position = position.checked_add(1).ok_or_else(|| invalid(path))?;
        if node.insert("sources".into(), json!([source])).is_some() {
            return Err(invalid(path));
        }
    }
    Ok(())
}

pub(in crate::ir::lowering) fn source_span(
    key: &CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    source: &ast::PublishedSourceUse,
) -> Result<SourceSpan, LoweringError> {
    let identity = key.identity.ok_or_else(|| invalid(path))?;
    Ok(SourceSpan {
        card_id: identity.card_id.clone(),
        card_no: key.key.to_owned(),
        face: source.face_id.clone(),
        owner: source.owner.to_string(),
        field: if entry.section.is_some() {
            "section"
        } else {
            "effect"
        }
        .into(),
        section: entry.section.map(|section| section.to_string()),
        line: entry.line,
        scope: if entry.keywords.is_some() {
            format!("keyword_declaration:{path}")
        } else {
            source.scope.clone()
        },
        fragment: None,
        author_path: path.to_owned(),
    })
}

pub(crate) fn lower_program(
    tree: &Value,
    number: &str,
    faces: &[ProgramFace],
    token_printings: &[String],
) -> Result<CurrentCardProgram, LoweringError> {
    let path = format!("/cards/{}", pointer_key(number));
    let mut program: Program = decode(tree.clone(), &path)?;
    let first = faces.first().ok_or_else(|| invalid(&path))?;
    if program.scope != "shared" || !super::permanent(&program.card_id, "c:", 32) {
        return Err(invalid(&path));
    }
    let mut identity = CurrentIdentity {
        card_id: program.card_id,
        faces: BTreeMap::new(),
    };
    let mut ordinals = BTreeMap::new();
    let mut first_positions = BTreeMap::new();
    bind_abilities(&mut program.abilities, first, &mut first_positions, &path)?;
    for (ordinal, face) in faces.iter().enumerate() {
        if !super::permanent(&face.face_id, "f:", 32)
            || identity
                .faces
                .insert(
                    face.revision_id.clone(),
                    (face.face_id.clone(), face.cost.map(Nat)),
                )
                .is_some()
            || ordinals.insert(face.face_id.clone(), ordinal).is_some()
        {
            return Err(invalid(&path));
        }
        if let Some(container) = program.faces.get_mut(&face.face_id) {
            let mut positions = if ordinal == 0 {
                take(&mut first_positions)
            } else {
                BTreeMap::new()
            };
            bind_abilities(&mut container.abilities, face, &mut positions, &path)?;
        }
    }
    if program.faces.keys().any(|id| !ordinals.contains_key(id)) {
        return Err(invalid(&path));
    }
    let coordinates = CardCoordinates {
        key: number,
        identity: Some(&identity),
    };
    let entries = decode_abilities(program.abilities, program.faces, &path)?;
    let mut sources = BTreeMap::new();
    let mut declarations = BTreeMap::new();
    for (entry_path, ability) in &entries {
        let ast::SourceUse::Published(reference) = &ability.sources[0] else {
            return Err(invalid(entry_path));
        };
        let source = source_span(&coordinates, entry_path, ability, reference)?;
        if let Some(keywords) = &ability.keywords {
            let keywords = keywords
                .iter()
                .map(|keyword| decode(Value::String(keyword.clone()), entry_path))
                .collect::<Result<Vec<_>, _>>()?;
            declarations.insert(
                entry_path.clone(),
                CurrentKeywordEvidence { source, keywords },
            );
        } else {
            sources.insert(entry_path.clone(), source);
        }
    }
    let lowered = lower_entries(&coordinates, &entries, None, &path)?;
    Ok(CurrentCardProgram {
        deck_rule: program.deck_rule,
        face_ordinals: ordinals,
        card_id: identity.card_id,
        card_no: number.into(),
        source_payload: json!({"faces": faces, "token_printings": token_printings}),
        expanded: json!({"cards": {(number): tree}}),
        token_printings: token_printings.to_vec(),
        sources,
        declarations,
        program: lowered,
    })
}
