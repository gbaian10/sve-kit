//! Committed history keeps its facts after a current object link retires.

use super::{ProjectionError, objects::ObjectScope};
use crate::game::player::{Viewer, observation as native};
use crate::ir::CardKind;
use core::result;
use sve_battle_contract::{Nat, Seat, Zone, observation as wire};

type Result<T> = result::Result<T, ProjectionError>;

/// Project committed public facts from one authority-produced observation.
///
/// Crate tests supply observations directly; hosts use `Game::battle_observation`.
/// Retired links remain absent.
///
/// # Errors
/// Refuses inconsistent object scopes, references outside the current scope,
/// and integers outside the wire's exact range.
#[cfg(all(test, feature = "runner"))]
pub(super) fn project_history(observation: &native::Observation) -> Result<wire::TurnHistory> {
    let objects = ObjectScope::new(observation)?;
    scoped(observation, &objects)
}

pub(super) fn scoped(
    observation: &native::Observation,
    objects: &ObjectScope<'_>,
) -> Result<wire::TurnHistory> {
    let history = &observation.history;
    Ok(wire::TurnHistory {
        subjects: history
            .subjects
            .iter()
            .map(|fact| subject(fact, objects))
            .collect::<Result<Vec<_>>>()?,
        damage: history
            .damage
            .iter()
            .map(|fact| {
                Ok(wire::DamageHistory {
                    recipient: objects.history_link(fact.recipient.as_ref())?,
                    source: objects.history_link(fact.source.as_ref())?,
                    controller: seat(fact.controller),
                })
            })
            .collect::<Result<Vec<_>>>()?,
        stat_gains: history
            .stat_gains
            .iter()
            .map(|fact| {
                Ok(wire::StatGainHistory {
                    current: objects.history_link(fact.current.as_ref())?,
                    attack: fact.attack,
                    life: fact.life,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        dice: history
            .dice
            .iter()
            .map(|fact| {
                Ok(wire::DiceHistory {
                    player: seat(fact.player),
                    value: natural(fact.value)?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        played: counts(history.played)?,
        union_bursts: counts(history.union_bursts)?,
    })
}

fn subject(
    fact: &native::SubjectHistory,
    objects: &ObjectScope<'_>,
) -> Result<wire::SubjectHistory> {
    Ok(wire::SubjectHistory {
        event: match fact.event {
            native::HistoryEvent::Play => wire::HistoryEvent::Play,
            native::HistoryEvent::Attack => wire::HistoryEvent::Attack,
            native::HistoryEvent::Evolve => wire::HistoryEvent::Evolve,
            native::HistoryEvent::Leave => wire::HistoryEvent::Leave,
        },
        current: objects.history_link(fact.current.as_ref())?,
        controller: seat(fact.controller),
        active: seat(fact.active),
        card_kind: match fact.kind {
            CardKind::Crest => wire::CardKind::Crest,
            CardKind::EvolveFollower => wire::CardKind::EvolveFollower,
            CardKind::Follower => wire::CardKind::Follower,
            CardKind::Spell => wire::CardKind::Spell,
            CardKind::Amulet => wire::CardKind::Amulet,
        },
        class: fact.class.clone(),
        traits: fact.traits.clone(),
        names: fact.names.clone(),
        token: fact.token,
        destroyed: fact.destroyed,
        destination: fact.destination.map(zone),
    })
}

const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}

const fn zone(value: native::ObservedZone) -> Zone {
    match value {
        native::ObservedZone::Deck => Zone::Deck,
        native::ObservedZone::EvolveDeck => Zone::EvolveDeck,
        native::ObservedZone::Hand => Zone::Hand,
        native::ObservedZone::Field => Zone::Field,
        native::ObservedZone::Ex => Zone::Ex,
        native::ObservedZone::Cemetery => Zone::Cemetery,
        native::ObservedZone::Banish => Zone::Banish,
        native::ObservedZone::Resolution => Zone::Resolution,
        native::ObservedZone::Evolution => Zone::Evolution,
        native::ObservedZone::Race => Zone::Race,
        native::ObservedZone::Drive => Zone::Drive,
        native::ObservedZone::Trigger => Zone::Trigger,
        native::ObservedZone::Equipment => Zone::Equipment,
    }
}

fn natural(value: u64) -> Result<Nat> {
    Nat::try_from(value).map_err(ProjectionError::from)
}

fn counts([first_seat, second_seat]: [u64; 2]) -> Result<[Nat; 2]> {
    Ok([natural(first_seat)?, natural(second_seat)?])
}
