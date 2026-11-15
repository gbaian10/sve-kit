use super::ProjectionError;
use crate::catalog::Catalog;
use crate::game::player::observation::CardFace;
use alloc::collections::{BTreeMap, BTreeSet};
use core::{fmt, result};
use serde::{Deserialize, Serialize};
use sve_battle_contract::CardRef;

type Result<T> = result::Result<T, ProjectionError>;

/// Trusted identity-only input keeps card facts in their sole pinned snapshot.
///
/// The host obtains this from the same validated snapshot reader as the native
/// catalogue. Player submissions cannot supply or replace these bindings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceInput {
    /// Exact region prevents cross-region card-number normalization.
    pub region: String,
    /// Exact native snapshot bytes prevent binding identities by display names.
    pub snapshot_sha256: String,
    /// The full native face closure prevents partial or guessed references.
    pub faces: Vec<PrintingFaceIdentity>,
}

/// Explicit coordinates avoid deriving a permanent printing from a native number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintingFaceIdentity {
    /// Exact native key preserves suffixes and official spelling.
    pub card_no: String,
    /// Exact native ordinal distinguishes faces of the same physical printing.
    pub native_face: u64,
    /// Published coordinates keep text and material lookup in the pinned snapshot.
    pub card: CardRef,
}

/// Immutable, complete printing references scoped to one validated catalogue.
#[derive(Clone)]
pub struct BattleReferences {
    region: String,
    snapshot_sha256: String,
    faces: BTreeMap<String, BTreeMap<u64, CardRef>>,
}

impl fmt::Debug for BattleReferences {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BattleReferences").finish_non_exhaustive()
    }
}

impl BattleReferences {
    /// Bind trusted snapshot identities without creating another card database.
    ///
    /// Legacy JP backgrounds are allowed because permanent IDs are supplied,
    /// never inferred from legacy numbers. Legacy native faces keep source order;
    /// adopted `source_face_map` entries can point to different permanent face ordinals.
    ///
    /// # Errors
    /// Refuses unbound or mismatched backgrounds, missing faces, aliases,
    /// collisions and coordinates outside either printing's face closure.
    pub fn new(catalog: &Catalog, input: ReferenceInput) -> Result<Self> {
        let ReferenceInput {
            region,
            snapshot_sha256,
            faces: identities,
        } = input;
        let mut references = Self {
            region,
            snapshot_sha256,
            faces: BTreeMap::new(),
        };
        references.validate_catalog(catalog)?;
        let mut printings = BTreeMap::new();
        let mut targets = BTreeSet::new();
        for identity in identities {
            references.bind_face(catalog, identity, &mut printings, &mut targets)?;
        }
        if catalog.cards.iter().any(|(number, card)| {
            card.faces.is_empty()
                || references
                    .faces
                    .get(number)
                    .is_none_or(|faces| faces.len() != card.faces.len())
        }) {
            return Err(ProjectionError::Unavailable);
        }
        Ok(references)
    }

    /// Require the current catalogue so native coordinates cannot cross snapshots.
    ///
    /// # Errors
    /// Refuses a mismatched catalogue or unknown card/face without exposing coordinates.
    pub fn resolve(&self, catalog: &Catalog, face: &CardFace) -> Result<CardRef> {
        self.validate_catalog(catalog)?;
        self.faces
            .get(&face.card)
            .and_then(|faces| faces.get(&face.face))
            .cloned()
            .ok_or(ProjectionError::Unavailable)
    }

    pub(crate) fn validate_catalog(&self, catalog: &Catalog) -> Result<()> {
        let Some((region, snapshot_sha256)) = catalog.rule_bindings.snapshot_background() else {
            return Err(ProjectionError::Unavailable);
        };
        if !matches!(region, "jp" | "en")
            || region != self.region
            || snapshot_sha256 != self.snapshot_sha256
        {
            return Err(ProjectionError::Unavailable);
        }
        Ok(())
    }

    fn bind_face(
        &mut self,
        catalog: &Catalog,
        identity: PrintingFaceIdentity,
        printings: &mut BTreeMap<String, String>,
        targets: &mut BTreeSet<CardRef>,
    ) -> Result<()> {
        let printed = catalog
            .cards
            .get(&identity.card_no)
            .ok_or(ProjectionError::Unavailable)?;
        if identity.card_no.is_empty()
            || identity.card.printing_id.is_empty()
            || printed.number != identity.card_no
            || usize::try_from(identity.native_face)
                .ok()
                .is_none_or(|face| face >= printed.faces.len())
            || usize::try_from(identity.card.face.get())
                .ok()
                .is_none_or(|face| face >= printed.faces.len())
            || !targets.insert(identity.card.clone())
        {
            return Err(ProjectionError::Unavailable);
        }
        if printings
            .insert(identity.card.printing_id.clone(), identity.card_no.clone())
            .is_some_and(|number| number != identity.card_no)
        {
            return Err(ProjectionError::Unavailable);
        }
        let faces = self.faces.entry(identity.card_no).or_default();
        if faces
            .values()
            .next()
            .is_some_and(|card| card.printing_id != identity.card.printing_id)
            || faces.insert(identity.native_face, identity.card).is_some()
        {
            return Err(ProjectionError::Unavailable);
        }
        Ok(())
    }
}
