//! Wire identities read from what an event froze and the game's pinned catalogue.

use super::{EventObjects, seat};
use crate::battle::{BattleReferences, ProjectionError};
use crate::catalog::Catalog;
use crate::game::player;
use crate::game::player::events::EventCard;
use crate::game::player::observation::{CardFace, EffectSource, TurnObservation};
use crate::ir::Phase;
use sve_battle_contract::event as wire;
use sve_battle_contract::observation::{self, TurnView};
use sve_battle_contract::{Nat, ObjectHandle, SourceRef};

/// The identities each event froze when it happened, as the wire names them.
///
/// The current board is never consulted: a card an event names may no longer be
/// identifiable, and a current object of the same printing is not that card.
#[derive(Clone, Copy, Debug)]
pub struct EventReferences<'pinned> {
    catalog: &'pinned Catalog,
    references: &'pinned BattleReferences,
}

impl<'pinned> EventReferences<'pinned> {
    /// Read events of a game pinned to `catalog`.
    ///
    /// # Errors
    /// The references were bound to another catalogue.
    pub fn new(
        catalog: &'pinned Catalog,
        references: &'pinned BattleReferences,
    ) -> Result<Self, ProjectionError> {
        references.validate_catalog(catalog)?;
        Ok(Self {
            catalog,
            references,
        })
    }

    fn face(&self, card: &str, face: u64) -> Result<sve_battle_contract::CardRef, ProjectionError> {
        self.references.resolve(
            self.catalog,
            &CardFace {
                card: card.into(),
                face,
            },
        )
    }
}

/// The encoding of the viewer's object table keys, so a client finds the object an event
/// names on the board it held then.
fn object(handle: &player::ObjectHandle) -> ObjectHandle {
    ObjectHandle::new(handle.as_str().into())
}

impl EventObjects for EventReferences<'_> {
    fn card(&self, card: &EventCard) -> Result<wire::EventCard, ProjectionError> {
        Ok(wire::EventCard {
            card: self.face(&card.properties.card, card.properties.face)?,
            object: card.handle.as_ref().map(object),
        })
    }

    fn source(&self, source: &EffectSource) -> Result<SourceRef, ProjectionError> {
        Ok(SourceRef::Card {
            printing_id: self.references.printing(self.catalog, &source.card)?.into(),
            face: source
                .face
                .map(|face| self.face(&source.card, face).map(|card| card.face))
                .transpose()?,
            line: source.line.map(Nat::try_from).transpose()?,
            // Kept as the event froze it: by now the holder may have left the board.
            holder: source.holder.as_ref().map(object),
        })
    }

    fn turn(&self, turn: &TurnObservation) -> Result<TurnView, ProjectionError> {
        let [first, second] = turn.elapsed_turns;
        Ok(TurnView {
            active: turn.active.map(seat),
            first_player: turn.first_player.map(seat),
            phase: turn.phase.map(|phase| match phase {
                Phase::Start => observation::Phase::Start,
                Phase::Main => observation::Phase::Main,
                Phase::End => observation::Phase::End,
            }),
            elapsed_turns: [Nat::try_from(first)?, Nat::try_from(second)?],
        })
    }
}
