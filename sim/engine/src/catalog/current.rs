//! Resolve current permanent owners against the exact snapshot's complete face set.
use super::Catalog;
use crate::ir::lowering::CurrentCardProgram;
use crate::ir::{FaceId, SourceSpan};
use crate::{Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;

#[derive(Debug)]
pub(crate) struct CurrentCard {
    pub(crate) candidate: Arc<CurrentCardProgram>,
    faces: BTreeMap<String, usize>,
}

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
        let faces = candidate.face_ordinals.clone();
        if faces.len() != card.faces.len()
            || faces.values().copied().collect::<BTreeSet<_>>() != (0..card.faces.len()).collect()
            || self.programs.contains_key(&candidate.card_no)
            || self.rejected.contains_key(&candidate.card_no)
        {
            return Err(invalid("current snapshot closure or prototype overlap"));
        }
        Ok(CurrentCard { candidate, faces })
    }
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

    pub(crate) fn check_source(&self, source: &SourceSpan, face: &str, line: u64) -> Result<()> {
        if source.face != face || u64::from(source.line) != line {
            return Err(invalid("current printed source coordinate mismatch"));
        }
        let expected = self
            .candidate
            .source_at(&source.author_path)
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
