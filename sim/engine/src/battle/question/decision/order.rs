use super::candidates::Candidates;
use super::{AnswerError, Context, Result};
use crate::battle::ProjectionError;
use crate::battle::question::common::seat;
use crate::game::player::observation::ObservedZone;
use crate::game::player::{self, OrderQuestion};
use crate::ir::Position;
use core::result;
use sve_battle_contract::{Answer, Zone, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    candidates: Candidates,
}

pub(super) fn project(
    domain: &OrderQuestion,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
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
        super::Binding::Order(Binding { candidates }),
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Order { order } = answer else {
            return Err(AnswerError);
        };
        if order.len() != self.candidates.values.len() {
            return Err(AnswerError);
        }
        Ok(player::Answer::Order(self.candidates.lookup(order)?))
    }
}
