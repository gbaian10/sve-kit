//! Resolve current permanent owners against the exact snapshot's complete face set.
use super::Catalog;
use crate::ir::lowering::CurrentCardProgram;
use crate::ir::{FaceId, SourceSpan};
use crate::{Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use serde_json::Value;

#[derive(Debug)]
pub(crate) struct CurrentCard {
    pub(crate) candidate: Arc<CurrentCardProgram>,
    faces: BTreeMap<String, usize>,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Current-source resolution stays beside the snapshot hash check it owns."
)]
impl Catalog {
    pub(crate) fn check_current_printing(&self, number: &str, current: &CurrentCard) -> Result<()> {
        let card = self
            .cards
            .get(number)
            .ok_or_else(|| invalid("current printing missing"))?;
        if card.faces.len() != current.faces.len()
            || self.programs.contains_key(number)
            || self.rejected.contains_key(number)
        {
            return Err(invalid("current snapshot closure or prototype overlap"));
        }
        Ok(())
    }

    pub(crate) fn resolve_current(
        &self,
        candidate: Arc<CurrentCardProgram>,
    ) -> Result<CurrentCard> {
        let card = self
            .cards
            .get(&candidate.card_no)
            .ok_or_else(|| invalid("current card missing from snapshot"))?;
        if candidate.source_version == "direct" {
            let faces = candidate.face_ordinals.clone();
            if faces.len() != card.faces.len()
                || faces.values().copied().collect::<BTreeSet<_>>()
                    != (0..card.faces.len()).collect()
                || self.programs.contains_key(&candidate.card_no)
                || self.rejected.contains_key(&candidate.card_no)
            {
                return Err(invalid("current snapshot closure or prototype overlap"));
            }
            return Ok(CurrentCard { candidate, faces });
        }
        let payload = candidate
            .source_payload
            .get("faces")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid("missing current faces"))?;
        if payload.len() != card.faces.len()
            || self.programs.contains_key(&candidate.card_no)
            || self.rejected.contains_key(&candidate.card_no)
        {
            return Err(invalid("current snapshot closure or prototype overlap"));
        }
        let mut faces = BTreeMap::new();
        let mut ordinals = BTreeSet::new();
        for face in payload {
            let id = face
                .get("face_id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("missing current face identity"))?;
            let matches = card
                .faces
                .iter()
                .enumerate()
                .filter(|(_, printed)| same_texts(face, printed))
                .map(|(ordinal, _)| ordinal)
                .collect::<Vec<_>>();
            let [ordinal] = matches.as_slice() else {
                return Err(invalid("current face has no unique snapshot source"));
            };
            if !ordinals.insert(*ordinal) || faces.insert(id.into(), *ordinal).is_some() {
                return Err(invalid("current face mapping is not bijective"));
            }
        }
        Ok(CurrentCard { candidate, faces })
    }
}

fn same_texts(source: &Value, printed: &Value) -> bool {
    let Some(texts) = source.get("texts").and_then(Value::as_array) else {
        return false;
    };
    let sections = printed
        .get("sections")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if texts.len() != sections.saturating_add(2) {
        return false;
    }
    for (attribute, field) in [("cost", "cost"), ("attack", "power"), ("defense", "hp")] {
        let Some(expected) = source.pointer(&format!("/attributes/{attribute}/value")) else {
            return false;
        };
        let actual = printed
            .get(field)
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<u64>().ok());
        if !expected.is_null() && expected.as_u64() != actual {
            return false;
        }
    }
    texts.iter().all(|text| {
        let value = match text.get("field").and_then(Value::as_str) {
            Some("name") => printed.get("name"),
            Some("effect") => printed.get("text"),
            Some("section") => text
                .get("ordinal")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| {
                    let list = printed.get("sections")?;
                    list.get(n)
                }),
            _ => return false,
        }
        .and_then(Value::as_str);
        match (text.get("state").and_then(Value::as_str), value) {
            (Some("present"), Some(value)) => {
                text.get("source_hash").and_then(Value::as_str)
                    == Some(super::rule_bindings::digest(value.as_bytes()).as_str())
            }
            (Some("absent"), None | Some("")) => {
                text.get("field").and_then(Value::as_str) == Some("effect")
                    && text.get("source_hash").and_then(Value::as_str)
                        == Some(super::rule_bindings::digest(b"").as_str())
            }
            (Some("unknown"), None | Some("")) => true,
            _ => false,
        }
    })
}

impl CurrentCard {
    pub(crate) fn ordinal(&self, face: &FaceId) -> Result<usize> {
        self.faces
            .get(&face.0)
            .copied()
            .ok_or_else(|| invalid("foreign current face"))
    }

    pub(crate) fn face(&self, ordinal: usize) -> Result<FaceId> {
        self.faces
            .iter()
            .find(|(_, n)| **n == ordinal)
            .map(|(id, _)| FaceId(id.clone()))
            .ok_or_else(|| invalid("unknown current face ordinal"))
    }

    pub(crate) fn check_source(&self, source: &SourceSpan) -> Result<()> {
        let expected = self
            .candidate
            .source_at(&source.author_path)
            .map(|proof| &proof.source)
            .or_else(|| {
                self.candidate
                    .declaration_at(&source.author_path)
                    .map(|proof| &proof.source)
            })
            .ok_or_else(|| invalid("missing current runtime source proof"))?;
        if source.card_id != self.candidate.card_id
            || source.card_no != self.candidate.card_no
            || source.face != expected.face
            || source.owner != expected.owner
            || source.field != expected.field
            || source.line != expected.line
            || source.section != expected.section
            || source.fragment != expected.fragment
            || source.scope != expected.scope
        {
            return Err(invalid("current runtime source mapping mismatch"));
        }
        Ok(())
    }
}
