//! Rule locations do not establish their applicability or completeness.

use super::ProjectionError;
use super::references::valid_manifest_hash;
use alloc::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use sve_battle_contract::{RuleRef, RuleSource, RuleStrength};

/// Identity closure from the same verified public manifest as the native catalogue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleInput {
    /// Exact public manifest digest.
    pub manifest_sha256: String,
    /// All published CR clauses, QA versions and ruling revisions in this closure.
    pub entries: Vec<RuleIdentity>,
}

/// Exact published coordinates, independent of a particular use's dependencies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleIdentity {
    /// A clause number in one exact comprehensive-rules version.
    Cr {
        /// Published clause number.
        id: String,
        /// Public `cr_version.id`, never its human version label.
        revision: String,
    },
    /// An answer's permanent identity and exact version.
    Qa {
        /// Public `qa.id`, never its optional official number.
        id: String,
        /// Public `qa_version.id`.
        revision: String,
    },
    /// A registered ruling revision retains its recorded strength.
    Ruling {
        /// Public `ruling_id`.
        id: String,
        /// Public `ruling_revision.id`.
        revision: String,
        /// Strength recorded on this revision.
        strength: RuleStrength,
    },
}

#[derive(Clone)]
pub(super) struct Rules {
    pub(super) manifest_sha256: String,
    entries: BTreeMap<(&'static str, String, String), Option<RuleStrength>>,
}

impl Rules {
    pub(super) fn new(input: RuleInput) -> Result<Self, ProjectionError> {
        if !valid_manifest_hash(&input.manifest_sha256) {
            return Err(ProjectionError::Unavailable);
        }
        let mut entries = BTreeMap::new();
        let mut parents = BTreeMap::new();
        for entry in input.entries {
            let (kind, id, revision, strength) = match entry {
                RuleIdentity::Cr { id, revision } => ("cr", id, revision, None),
                RuleIdentity::Qa { id, revision } => ("qa", id, revision, None),
                RuleIdentity::Ruling {
                    id,
                    revision,
                    strength,
                } => ("ruling", id, revision, Some(strength)),
            };
            if id.is_empty()
                || revision.is_empty()
                // A CR version owns many clauses; a QA or ruling revision owns one parent.
                || (kind != "cr"
                    && parents
                        .insert((kind, revision.clone()), id.clone())
                        .is_some_and(|old| old != id))
                || entries.insert((kind, id, revision), strength).is_some()
            {
                return Err(ProjectionError::Unavailable);
            }
        }
        Ok(Self {
            manifest_sha256: input.manifest_sha256,
            entries,
        })
    }

    pub(super) fn validate(&self, reference: &RuleRef) -> Result<(), ProjectionError> {
        let kind = match reference.kind {
            RuleSource::Cr => "cr",
            RuleSource::Qa => "qa",
            RuleSource::Ruling => "ruling",
        };
        let strength = self
            .entries
            .get(&(kind, reference.id.clone(), reference.revision.clone()))
            .ok_or(ProjectionError::Unavailable)?;
        if strength.is_some_and(|strength| strength != reference.strength) {
            return Err(ProjectionError::Unavailable);
        }
        Ok(())
    }
}
