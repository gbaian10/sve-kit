//! Exact sources require admitted provenance and the pinned wording identity closure.

use super::{BattleReferences, ProjectionError, objects::ObjectScope};
use crate::catalog::Catalog;
use crate::game::player::observation::{CardFace, EffectSource, TextProvenance, WordingField};
use core::result;
use sve_battle_contract::{Nat, ObjectHandle, SourceRef, TextRef};

type Result<T> = result::Result<T, ProjectionError>;

pub(super) fn source(
    catalog: &Catalog,
    references: &BattleReferences,
    objects: &ObjectScope<'_>,
    origin: &EffectSource,
) -> Result<SourceRef> {
    source_ref(
        catalog,
        references,
        origin,
        objects.optional(origin.holder.as_ref())?,
    )
}

pub(crate) fn source_ref(
    catalog: &Catalog,
    references: &BattleReferences,
    origin: &EffectSource,
    holder: Option<ObjectHandle>,
) -> Result<SourceRef> {
    if origin.face.is_none() && origin.line.is_some() {
        return Err(ProjectionError::Unavailable);
    }
    let printing_id = references.printing(catalog, &origin.card)?.into();
    let face = origin
        .face
        .map(|face| {
            references
                .resolve(
                    catalog,
                    &CardFace {
                        card: origin.card.clone(),
                        face,
                    },
                )
                .map(|card| card.face)
        })
        .transpose()?;
    if let Some(text) = exact_text(catalog, references, origin, holder.clone())? {
        return Ok(SourceRef::Text { text });
    }
    Ok(SourceRef::Card {
        printing_id,
        face,
        line: origin
            .line
            .map(Nat::try_from)
            .transpose()
            .map_err(ProjectionError::from)?,
        holder,
    })
}

fn exact_text(
    catalog: &Catalog,
    references: &BattleReferences,
    origin: &EffectSource,
    holder: Option<ObjectHandle>,
) -> Result<Option<TextRef>> {
    let giver = match &origin.provenance {
        TextProvenance::Unknown => return Ok(None),
        TextProvenance::Printed => None,
        TextProvenance::Granted { by } => Some(by),
    };
    let Some(wording) = origin.wording.as_ref().filter(|_| references.has_wording()) else {
        return Ok(None);
    };
    let face = origin.face.ok_or(ProjectionError::Unavailable)?;
    let line = origin
        .line
        .filter(|line| *line > 0)
        .ok_or(ProjectionError::Unavailable)?;
    let card = references.validate_wording(
        catalog,
        &CardFace {
            card: origin.card.clone(),
            face,
        },
        wording,
    )?;
    let section = match wording.field {
        WordingField::Effect => "effect".into(),
        WordingField::Section { ordinal } => format!("section:{}", Nat::try_from(ordinal)?.get()),
    };
    Ok(Some(TextRef {
        card,
        wording_revision: wording.revision_id.clone(),
        section,
        line: Nat::try_from(line)?,
        step: None,
        granted_by: giver
            .map(|giver| references.resolve(catalog, giver))
            .transpose()?,
        holder,
    }))
}
