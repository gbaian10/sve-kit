//! Known printing origins stay usable without inventing exact wording references.

use super::{BattleReferences, ProjectionError, objects::ObjectScope};
use crate::catalog::Catalog;
use crate::game::player::observation::{CardFace, EffectSource};
use core::result;
use sve_battle_contract::{Nat, SourceRef};

type Result<T> = result::Result<T, ProjectionError>;

pub(super) fn source(
    catalog: &Catalog,
    references: &BattleReferences,
    objects: &ObjectScope<'_>,
    origin: &EffectSource,
) -> Result<SourceRef> {
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
    Ok(SourceRef::Card {
        printing_id,
        face,
        line: origin
            .line
            .map(Nat::try_from)
            .transpose()
            .map_err(ProjectionError::from)?,
        holder: objects.optional(origin.holder.as_ref())?,
    })
}
