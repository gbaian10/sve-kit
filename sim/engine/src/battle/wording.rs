//! Wording identities belong to the same verified public manifest as native facts.

use super::{BattleReferences, ProjectionError};
use crate::catalog::Catalog;
use crate::game::player::observation::{CardFace, WordingField, WordingSource};
use alloc::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Serialize};
use sve_battle_contract::Nat;

/// Trusted identity-only wording closure from the validated public snapshot reader.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordingInput {
    /// Hash of the exact public manifest used to produce the native snapshot.
    pub manifest_sha256: String,
    /// Every native face, including faces outside selected programs and decks.
    pub faces: Vec<WordingFaceIdentity>,
}

/// Permanent wording ownership of one native printing face.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordingFaceIdentity {
    /// Unmodified native printing key.
    pub card_no: String,
    /// Face in the exact native snapshot.
    pub native_face: u64,
    /// Permanent face owning every listed revision.
    pub face_id: String,
    /// All wording revisions for this face and region in the public closure.
    pub revisions: Vec<WordingRevisionIdentity>,
}

/// Fields that exist on one exact wording revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordingRevisionIdentity {
    /// Exact public wording owner, never the current revision substituted for it.
    pub revision_id: String,
    /// Located original fields; empty does not provide an ability source.
    pub fields: Vec<WordingFieldIdentity>,
}

/// An original field whose existence the snapshot reader verified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WordingFieldIdentity {
    /// Main original effect text.
    #[expect(
        clippy::empty_enum_variants_with_brackets,
        reason = "Serde only rejects fields beside this tag when it is a struct variant."
    )]
    Effect {},
    /// Original section in the owner's authoritative order.
    Section {
        /// Exact section ordinal.
        ordinal: Nat,
    },
}

#[derive(Clone)]
struct FaceWording {
    face_id: String,
    revisions: BTreeMap<String, BTreeSet<WordingFieldIdentity>>,
}

#[derive(Clone)]
pub(super) struct Wordings {
    pub(super) manifest_sha256: String,
    faces: BTreeMap<(String, u64), FaceWording>,
}

impl Wordings {
    pub(super) fn new(
        catalog: &Catalog,
        references: &BattleReferences,
        input: WordingInput,
        expected_faces: usize,
    ) -> Result<Self, ProjectionError> {
        let Some(hash) = input.manifest_sha256.strip_prefix("sha256:") else {
            return Err(ProjectionError::Unavailable);
        };
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ProjectionError::Unavailable);
        }
        let mut result = Self {
            manifest_sha256: input.manifest_sha256,
            faces: BTreeMap::new(),
        };
        let mut permanent_faces = BTreeSet::new();
        let mut owners = BTreeMap::new();
        for face in input.faces {
            let card = references.resolve(
                catalog,
                &CardFace {
                    card: face.card_no.clone(),
                    face: face.native_face,
                },
            )?;
            if face.face_id.is_empty()
                || !permanent_faces.insert((card.printing_id, face.face_id.clone()))
            {
                return Err(ProjectionError::Unavailable);
            }
            let mut revisions = BTreeMap::new();
            for revision in face.revisions {
                let field_count = revision.fields.len();
                let fields: BTreeSet<_> = revision.fields.into_iter().collect();
                if revision.revision_id.is_empty()
                    || fields.len() != field_count
                    || owners
                        .insert(revision.revision_id.clone(), face.face_id.clone())
                        .is_some_and(|previous| previous != face.face_id)
                    || revisions.insert(revision.revision_id, fields).is_some()
                {
                    return Err(ProjectionError::Unavailable);
                }
            }
            if result
                .faces
                .insert(
                    (face.card_no, face.native_face),
                    FaceWording {
                        face_id: face.face_id,
                        revisions,
                    },
                )
                .is_some()
            {
                return Err(ProjectionError::Unavailable);
            }
        }
        if result.faces.len() != expected_faces {
            return Err(ProjectionError::Unavailable);
        }
        Ok(result)
    }

    pub(super) fn validate(
        &self,
        face: &CardFace,
        source: &WordingSource,
    ) -> Result<(), ProjectionError> {
        let field = match source.field {
            WordingField::Effect => WordingFieldIdentity::Effect {},
            WordingField::Section { ordinal } => WordingFieldIdentity::Section {
                ordinal: Nat::try_from(ordinal)?,
            },
        };
        let valid = self
            .faces
            .get(&(face.card.clone(), face.face))
            .is_some_and(|wording| {
                wording.face_id == source.face_id
                    && wording
                        .revisions
                        .get(&source.revision_id)
                        .is_some_and(|fields| fields.contains(&field))
            });
        if valid {
            Ok(())
        } else {
            Err(ProjectionError::Unavailable)
        }
    }
}
