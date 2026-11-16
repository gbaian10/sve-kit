use super::{AnswerError, Binding, Result, seat};
use crate::battle::{CandidateDetails, ProjectionError, SelectionProjection};
use crate::game::player::{self, EvolutionAnswer, Viewer};
use alloc::collections::BTreeMap;
use core::result;
use sve_battle_contract::{
    Answer, QuestionId, Subject, Zone, observation::BattleObservation, question as wire,
};

pub(super) fn discard(
    id: &QuestionId,
    actor: Viewer,
    domain: &player::SelectionQuestion,
    observation: &BattleObservation,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    // The native answer carries only candidate coordinates, so role assignments cannot be dropped.
    if !domain.roles.is_empty()
        || u64::try_from(domain.members.len())
            .ok()
            .is_none_or(|count| domain.cardinality.minimum.0 > count)
    {
        return Err(ProjectionError::Unavailable);
    }
    let hand = &observation
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .ok_or(ProjectionError::Unavailable)?
        .zones
        .hand;
    let projection = SelectionProjection::new(id, domain, &BTreeMap::new(), |coordinate| {
        let member = domain
            .members
            .iter()
            .find(|member| member.handle == coordinate && member.player == actor)
            .ok_or(ProjectionError::Unavailable)?;
        let handle = member.object.as_ref().ok_or(ProjectionError::Unavailable)?;
        let located = hand
            .cards
            .iter()
            .find(|card| card.object.as_str() == handle.as_str())
            .ok_or(ProjectionError::Unavailable)?;
        let object = observation
            .objects
            .get(&located.object)
            .ok_or(ProjectionError::Unavailable)?;
        Ok(CandidateDetails {
            subject: Some(Subject::Card {
                card: object.effective_face.clone(),
                object: Some(located.object.clone()),
            }),
            controller: Some(object.controller),
            origin: Some(wire::Origin {
                player: seat(actor),
                zone: Zone::Hand,
                position: located.position,
            }),
            // This domain has neither a class-distinct constraint nor captured class facts.
            classes: None,
        })
    })?;
    Ok((
        wire::QuestionUse::EvolutionDiscard,
        wire::QuestionShape::Select {
            selection: projection.selection().clone(),
        },
        Binding::EvolutionDiscard(projection),
    ))
}

pub(super) fn discard_answer(
    projection: &SelectionProjection,
    answer: &Answer,
) -> result::Result<EvolutionAnswer, AnswerError> {
    let Answer::Select { selected, .. } = answer else {
        return Err(AnswerError);
    };
    let count = u64::try_from(selected.len()).map_err(|_range| AnswerError)?;
    let bounds = projection.selection().bounds;
    if count < bounds.min.get() || bounds.max.is_some_and(|max| count > max.get()) {
        return Err(AnswerError);
    }
    let player::Answer::Targets(handles) = projection
        .targets(answer)
        .map_err(|_projection| AnswerError)?
    else {
        return Err(AnswerError);
    };
    Ok(EvolutionAnswer::Discard(handles))
}
