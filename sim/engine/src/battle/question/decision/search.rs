use super::{AnswerError, Context, Result};
use crate::battle::question::common::seat;
use crate::battle::{CandidateDetails, ProjectionError, SelectionProjection};
use crate::game::player::observation::ObservedZone;
use crate::game::player::{self, SearchFacts, SearchQuestion, SelectionMember};
use alloc::boxed::Box;
use core::result;
use sve_battle_contract::{Answer, Subject, Zone, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    projection: SelectionProjection,
}

pub(super) fn project(
    domain: &SearchQuestion,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let facts = domain.facts.as_ref().ok_or(ProjectionError::Unavailable)?;
    // Native admission permits searching only the actor's own deck.
    if context.observation.viewer != seat(context.actor)
        || facts.origin.player != context.actor
        || facts.origin.zone != ObservedZone::Deck
        || facts.faces.len() != domain.cards.len()
        || u64::try_from(domain.cards.len())
            .ok()
            .is_none_or(|count| domain.cardinality.minimum.0 > count)
    {
        return Err(ProjectionError::Unavailable);
    }
    let projection = SelectionProjection::search(context.id, domain, |coordinate| {
        details(domain, facts, coordinate, context)
    })?;
    Ok((
        wire::QuestionUse::Search,
        wire::QuestionShape::Select {
            selection: projection.selection().clone(),
        },
        super::Binding::Search(Box::new(Binding { projection })),
    ))
}

fn details(
    domain: &SearchQuestion,
    facts: &SearchFacts,
    coordinate: u64,
    context: Context<'_>,
) -> Result<CandidateDetails> {
    let option = domain
        .cards
        .iter()
        .find(|card| card.handle == coordinate)
        .ok_or(ProjectionError::Unavailable)?;
    let face = facts
        .faces
        .get(&coordinate)
        .filter(|face| face.card == option.card)
        .ok_or(ProjectionError::Unavailable)?;
    let card = context.references.resolve(context.catalog, face)?;
    let object = if let Some(handle) = &option.object {
        let member = SelectionMember {
            handle: coordinate,
            player: facts.origin.player,
            card: Some(option.card.clone()),
            name: Some(option.name.clone()),
            object: Some(handle.clone()),
            rules_names: option.rules_names.clone(),
            original_cost: option.original_cost,
        };
        let linked = super::candidates::details(&member, context)?;
        if linked.controller != Some(seat(facts.origin.player))
            || linked.origin.is_none_or(|origin| {
                origin.player != seat(facts.origin.player) || origin.zone != Zone::Deck
            })
        {
            return Err(ProjectionError::Unavailable);
        }
        let Some(Subject::Card {
            card: linked_card,
            object: Some(object),
        }) = linked.subject
        else {
            return Err(ProjectionError::Unavailable);
        };
        if linked_card != card {
            return Err(ProjectionError::Unavailable);
        }
        Some(object)
    } else {
        // No authorized current object exists to reference for this printing-only candidate.
        None
    };
    Ok(CandidateDetails {
        subject: Some(Subject::Card { card, object }),
        controller: Some(seat(facts.origin.player)),
        // A search exposes a set; even a known candidate does not disclose a deck slot here.
        origin: Some(wire::Origin {
            player: seat(facts.origin.player),
            zone: Zone::Deck,
            position: None,
        }),
        classes: None,
    })
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Select { selected, .. } = answer else {
            return Err(AnswerError);
        };
        let count = u64::try_from(selected.len()).map_err(|_range| AnswerError)?;
        let bounds = self.projection.selection().bounds;
        if count < bounds.min.get() || bounds.max.is_some_and(|max| count > max.get()) {
            return Err(AnswerError);
        }
        self.projection
            .search_answer(answer)
            .map(player::Answer::Search)
            .map_err(|_projection| AnswerError)
    }
}
