use super::{EventCard, EventError, EventKind, EventLocation, EventSubject};
use crate::game::player::observation::ObservedZone;
use crate::game::player::{ObjectHandle, Viewer};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CardKey {
    card: String,
    face: u64,
    names: Vec<String>,
    kind: String,
    traits: Vec<String>,
    owner: Viewer,
    controller: Viewer,
    handle: Option<ObjectHandle>,
}

impl From<&EventCard> for CardKey {
    fn from(card: &EventCard) -> Self {
        let properties = &card.properties;
        Self {
            card: properties.card.clone(),
            face: properties.face,
            names: properties.names.clone(),
            kind: properties.kind.clone(),
            traits: properties.traits.clone(),
            owner: properties.owner,
            controller: properties.controller,
            handle: card.handle.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LocationKey {
    player: Viewer,
    zone: u8,
    position: Option<u64>,
    card: Option<CardKey>,
}

impl From<&EventLocation> for LocationKey {
    fn from(location: &EventLocation) -> Self {
        Self {
            player: location.player,
            zone: zone(location.zone),
            position: location.position,
            card: location.card.as_ref().map(CardKey::from),
        }
    }
}

const fn zone(zone: Option<ObservedZone>) -> u8 {
    match zone {
        None => 0,
        Some(ObservedZone::Deck) => 1,
        Some(ObservedZone::EvolveDeck) => 2,
        Some(ObservedZone::Hand) => 3,
        Some(ObservedZone::Field) => 4,
        Some(ObservedZone::Ex) => 5,
        Some(ObservedZone::Cemetery) => 6,
        Some(ObservedZone::Banish) => 7,
        Some(ObservedZone::Resolution) => 8,
        Some(ObservedZone::Evolution) => 9,
        Some(ObservedZone::Race) => 10,
        Some(ObservedZone::Drive) => 11,
        Some(ObservedZone::Trigger) => 12,
        Some(ObservedZone::Equipment) => 13,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum EventKey {
    Hidden,
    Movement(Box<(LocationKey, LocationKey)>),
    Disclosure(Vec<Option<CardKey>>),
    Fixed,
}

pub(super) fn key(view: &Result<Option<EventKind>, EventError>) -> EventKey {
    match view {
        Ok(None) => EventKey::Hidden,
        Ok(Some(EventKind::Movement { before, after, .. })) => EventKey::Movement(Box::new((
            LocationKey::from(before.as_ref()),
            LocationKey::from(after.as_ref()),
        ))),
        Ok(Some(EventKind::Action(action))) => EventKey::Disclosure(
            action
                .subjects
                .iter()
                .map(|subject| match subject {
                    Some(EventSubject::Card(card)) => Some(CardKey::from(card.as_ref())),
                    Some(EventSubject::Leader(_)) | None => None,
                })
                .collect(),
        ),
        Ok(Some(EventKind::Phase(_) | EventKind::Result { .. })) | Err(_) => EventKey::Fixed,
    }
}
