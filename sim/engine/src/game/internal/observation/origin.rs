//! Frozen source identity uses the same knowledge periods as existing object links.

use super::{Game, ObjectLink};
use crate::game::player::Viewer;
use crate::game::player::observation::{CardFace, TextProvenance};
use crate::ir::{Generation, ObjectId, ObjectRef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::game::internal) struct SourceIdentity {
    card: String,
    face: u64,
    link: ObjectLink,
    publicly_identified: bool,
}

impl SourceIdentity {
    pub(in crate::game::internal) fn holder(
        game: &Game,
        holder: &str,
        generation: u64,
    ) -> Option<Self> {
        Self::capture(
            game,
            ObjectRef {
                id: ObjectId(*game.state.ir_objects.get(holder)?),
                generation: Generation(generation),
            },
        )
    }

    pub(in crate::game::internal) fn capture(game: &Game, reference: ObjectRef) -> Option<Self> {
        let (holder, _) = game
            .state
            .ir_objects
            .iter()
            .find(|(_, id)| **id == reference.id.0)?;
        let object = game.object(holder).ok()?;
        if object.generation != reference.generation.0 {
            return None;
        }
        let (card, face) = game.internal_effective_print(holder).ok()?;
        let physical = game.information_source(holder).ok()?;
        Some(Self {
            card,
            face: u64::try_from(face).ok()?,
            link: ObjectLink::capture(game, &physical.id, physical.generation),
            publicly_identified: game.player_knows_object(Viewer::P1, &physical.id)
                && game.player_knows_object(Viewer::P2, &physical.id),
        })
    }

    pub(in crate::game::internal) fn matches(&self, card: &str, face: u64) -> bool {
        self.card == card && self.face == face
    }

    pub(in crate::game::internal) fn observed(&self, viewer: Viewer) -> Option<CardFace> {
        (self.publicly_identified || self.link.period(viewer).is_some()).then(|| CardFace {
            card: self.card.clone(),
            face: self.face,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::game::internal) enum TextOrigin {
    #[default]
    Unknown,
    Printed,
    Granted(SourceIdentity),
}

impl TextOrigin {
    pub(in crate::game::internal) fn observed(&self, viewer: Viewer) -> TextProvenance {
        match self {
            Self::Unknown => TextProvenance::Unknown,
            Self::Printed => TextProvenance::Printed,
            Self::Granted(giver) => giver
                .observed(viewer)
                .map_or(TextProvenance::Unknown, |by| TextProvenance::Granted { by }),
        }
    }
}
