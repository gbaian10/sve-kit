//! Resolve permanent identities and complete direct author coordinates.

use super::{LoweringError, Nat, SourceSpan, WellFormedCardProgram, error};
use alloc::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) mod direct;

/// Current source facts and their lowered candidate, without use qualification.
#[expect(
    clippy::partial_pub_fields,
    reason = "Callers resolve checked sources through the lookup methods."
)]
#[derive(Debug)]
pub struct CurrentCardProgram {
    /// Construction rule reconciled with the complete current-source handoff.
    pub deck_rule: Option<super::CardDeckRule>,
    /// Explicit face ordinals from the identity registry, used by direct admission.
    pub face_ordinals: BTreeMap<String, usize>,
    /// Permanent owner selected by the current reader's JP printing lookup.
    pub card_id: String,
    /// Unmodified JP printing coordinate.
    pub card_no: String,
    /// Complete current faces and explicit token selection, retained separately from IR.
    pub source_payload: Value,
    /// Expanded author tree and its direct author coordinates.
    pub expanded: Value,
    /// One current-checked source per ability, keyed by its author JSON pointer.
    sources: BTreeMap<String, SourceSpan>,
    /// Printed declaration evidence; an empty map for body-only cards.
    declarations: BTreeMap<String, CurrentKeywordEvidence>,
    /// Well-formed IR; this alone grants neither whole-card admission nor execution.
    pub program: WellFormedCardProgram,
}

/// A printed declaration, in source order.
#[expect(
    clippy::partial_pub_fields,
    reason = "The keyword order is exposed through a read-only slice."
)]
#[derive(Debug)]
pub struct CurrentKeywordEvidence {
    /// Permanent owner and original author location.
    pub source: SourceSpan,
    keywords: Vec<PrintedKeyword>,
}

impl CurrentKeywordEvidence {
    /// Ordered keywords retained without granting runtime support.
    #[must_use]
    pub fn keywords(&self) -> &[PrintedKeyword] {
        &self.keywords
    }
}

/// The current reader's closed printed-keyword subset; runtime support is separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintedKeyword {
    /// Printed Stack; entry and departure use the ordinary rule procedures.
    Stack,
    /// Quick declaration; the existing permission shape still controls runtime admission.
    Quick,
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
    /// Printed Drain.
    Drain,
    /// Printed Intimidate.
    Intimidate,
    /// Printed Assail.
    Assail,
    /// Printed single Drive check on attack.
    SingleDrive,
    /// Printed two sequential Drive checks on attack.
    TwinDrive,
    /// Printed Start Amulet; only the title opening reads it.
    StartAmulet,
    /// Printed eligibility for a final limit break.
    FinalLimitBreakPossible,
}

impl CurrentCardProgram {
    /// Resolve a printed declaration or its lowered keyword element separately from bodies.
    #[must_use]
    pub fn declaration_at(&self, mut author_path: &str) -> Option<&CurrentKeywordEvidence> {
        loop {
            if let Some(evidence) = self.declarations.get(author_path) {
                return Some(evidence);
            }
            (author_path, _) = author_path.rsplit_once('/')?;
        }
    }

    /// Resolve an expanded node or IR instruction's author path to its checked use.
    #[must_use]
    pub fn source_at(&self, mut author_path: &str) -> Option<&SourceSpan> {
        loop {
            if let Some(evidence) = self.sources.get(author_path) {
                return Some(evidence);
            }
            (author_path, _) = author_path.rsplit_once('/')?;
        }
    }
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
            .pointer("/owner/revision_id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid(path))?;
        self.faces
            .get(revision)
            .map(|(_, cost)| *cost)
            .ok_or_else(|| invalid(path))
    }
}
