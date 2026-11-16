use super::{AnswerError, NativeHandle, Result};
use crate::battle::ProjectionError;
use crate::game::player::{self, Viewer};
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{
    Answer, OptionId, QuestionId, Seat, Subject, Zone,
    observation::{BattleObservation, ZoneView},
    question as wire,
};

type CardCandidates = (Vec<wire::Candidate>, BTreeMap<OptionId, NativeHandle>);

pub(super) fn hand_candidates(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
) -> Result<CardCandidates> {
    let hand = &observation
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .ok_or(ProjectionError::Unavailable)?
        .zones
        .hand;
    if u64::try_from(cards.len()).ok() != Some(hand.count.get()) || cards.len() != hand.cards.len()
    {
        return Err(ProjectionError::Unavailable);
    }
    card_candidates(id, actor, cards, observation, Zone::Hand, hand)
}

pub(super) const fn plain_selection(
    candidates: Vec<wire::Candidate>,
    bounds: wire::Bounds,
) -> wire::QuestionShape {
    wire::QuestionShape::Select {
        selection: wire::Selection {
            candidates,
            shown: vec![],
            bounds,
            groups: vec![],
            group_assignment: wire::GroupAssignment::Existence,
            roles: vec![],
            distinct: vec![],
            original_cost_total: wire::CostTotal {
                min: None,
                max: None,
            },
            capacity: None,
        },
    }
}

pub(super) fn card_candidates(
    id: &QuestionId,
    actor: Viewer,
    cards: &[player::observation::CardObservation],
    observation: &BattleObservation,
    zone: Zone,
    members: &ZoneView,
) -> Result<CardCandidates> {
    let mut coordinates = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();
    for (index, card) in cards.iter().enumerate() {
        if !seen.insert(&card.handle) {
            return Err(ProjectionError::Unavailable);
        }
        let member = members
            .cards
            .iter()
            .find(|member| member.object.as_str() == card.handle.as_str())
            .ok_or(ProjectionError::Unavailable)?;
        let object = observation
            .objects
            .get(&member.object)
            .ok_or(ProjectionError::Unavailable)?;
        let option_id = option(id, index);
        candidates.push(wire::Candidate {
            id: option_id.clone(),
            subject: Some(Subject::Card {
                card: object.effective_face.clone(),
                object: Some(member.object.clone()),
            }),
            controller: Some(object.controller),
            origin: Some(wire::Origin {
                player: seat(actor),
                zone,
                position: member.position,
            }),
            rules_names: None,
            original_cost: None,
            classes: None,
        });
        coordinates.insert(option_id, card.handle.clone());
    }
    Ok((candidates, coordinates))
}

pub(super) fn selected_cards(
    cards: &BTreeMap<OptionId, NativeHandle>,
    exact: Option<usize>,
    answer: &Answer,
) -> result::Result<Vec<NativeHandle>, AnswerError> {
    let Answer::Select {
        selected,
        groups,
        roles,
    } = answer
    else {
        return Err(AnswerError);
    };
    if exact.is_some_and(|required| selected.len() != required)
        || groups.is_some()
        || !roles.is_empty()
    {
        return Err(AnswerError);
    }
    let mut seen = BTreeSet::new();
    let handles = selected
        .iter()
        .map(|choice| {
            if !seen.insert(choice) {
                return Err(AnswerError);
            }
            cards.get(choice).cloned().ok_or(AnswerError)
        })
        .collect::<result::Result<Vec<_>, _>>()?;
    Ok(handles)
}

pub(super) fn option(question: &QuestionId, index: usize) -> OptionId {
    OptionId::new(format!("{}:c{index}", question.as_str()))
}

pub(super) const fn seat(viewer: Viewer) -> Seat {
    match viewer {
        Viewer::P1 => Seat::P1,
        Viewer::P2 => Seat::P2,
    }
}
