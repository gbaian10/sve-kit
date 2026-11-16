use super::candidates::Candidates;
use super::{AnswerError, Context, Result};
use crate::battle::ProjectionError;
use crate::battle::question::common::seat;
use crate::game::player::observation::ObservedZone;
use crate::game::player::{self, OrderQuestion};
use crate::ir::Position;
use alloc::collections::BTreeMap;
use core::result;
use sve_battle_contract::{Answer, OptionId, Zone, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    coordinates: BTreeMap<OptionId, u64>,
}

pub(super) fn project(
    domain: &OrderQuestion,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    // Admission permits only the chooser's deck; broader admission must update this projection too.
    if domain.destination.player != context.actor || domain.destination.zone != ObservedZone::Deck {
        return Err(ProjectionError::Unavailable);
    }
    let (direction, edge) = match (domain.direction, domain.position) {
        (player::OrderDirection::TopDown, Position::Top {}) => {
            (wire::OrderDirection::TopDown, wire::Edge::Top)
        }
        (player::OrderDirection::TopDown, Position::Bottom {}) => {
            (wire::OrderDirection::TopDown, wire::Edge::Bottom)
        }
        (player::OrderDirection::TopDown, Position::Index { .. }) => {
            return Err(ProjectionError::Unavailable);
        }
    };
    let candidates = Candidates::cards(&domain.cards, domain.destination.player, context)?;
    if candidates.values.iter().any(|candidate| {
        candidate.origin.is_none_or(|origin| {
            origin.player != seat(domain.destination.player) || origin.zone != Zone::Deck
        })
    }) {
        return Err(ProjectionError::Unavailable);
    }
    let items = candidates
        .values
        .iter()
        .map(|candidate| wire::OrderItem {
            id: candidate.id.clone(),
            subject: candidate.subject.clone(),
            source: None,
        })
        .collect();
    Ok((
        wire::QuestionUse::OrderCards,
        wire::QuestionShape::Order {
            items,
            direction,
            destination: Some(wire::Destination {
                player: seat(domain.destination.player),
                zone: Zone::Deck,
                position: Some(edge),
            }),
        },
        super::Binding::Order(Binding::new(candidates.into_coordinates())),
    ))
}

impl Binding {
    pub(super) const fn new(coordinates: BTreeMap<OptionId, u64>) -> Self {
        Self { coordinates }
    }
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Order { order } = answer else {
            return Err(AnswerError);
        };
        if order.len() != self.coordinates.len() {
            return Err(AnswerError);
        }
        Ok(player::Answer::Order(super::candidates::lookup(
            &self.coordinates,
            order,
        )?))
    }
}
