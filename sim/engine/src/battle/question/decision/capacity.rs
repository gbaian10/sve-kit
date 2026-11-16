use super::{AnswerError, Context, Result};
use crate::battle::question::common::{option, seat};
use crate::battle::selection::zone;
use crate::battle::{CandidateDetails, ProjectionError, SelectionProjection};
use crate::game::player::observation::{CardFace, ObservedZone};
use crate::game::player::{
    self, CapacityCard, CardOption, MovementCapacityQuestion, SelectionMember,
    TokenCapacityQuestion,
};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::result;
use sve_battle_contract::{Answer, CardRef, Nat, OptionId, Subject, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) enum Binding {
    Movement(Box<SelectionProjection>),
    Tokens(BTreeMap<OptionId, u64>),
}

pub(super) fn tokens(
    domain: &TokenCapacityQuestion,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let faces = domain.faces.as_ref().ok_or(ProjectionError::Unavailable)?;
    let available = Nat::try_from(domain.available.0)?;
    if context.observation.viewer != seat(context.actor)
        || faces.len() != domain.choices.len()
        || domain.choices.is_empty()
        || available == Nat::ZERO
        || !matches!(domain.zone, ObservedZone::Field | ObservedZone::Ex)
    {
        return Err(ProjectionError::Unavailable);
    }
    let mut options = vec![];
    let mut coordinates = BTreeMap::new();
    for (index, (names, printings)) in domain.choices.iter().zip(faces).enumerate() {
        if names.is_empty()
            || names.iter().any(|name| name.trim().is_empty())
            || names.len() != printings.len()
            || u64::try_from(names.len())
                .ok()
                .is_none_or(|count| count > domain.available.0)
        {
            return Err(ProjectionError::Unavailable);
        }
        let tokens = names
            .iter()
            .zip(printings)
            .map(|(name, face)| {
                let card = created_card(face, context)?;
                let ordinal =
                    usize::try_from(face.face).map_err(|_range| ProjectionError::Range)?;
                // Validate this frozen printing's label; never choose another printing by that label.
                if context
                    .catalog
                    .rules_label(&face.card, ordinal)
                    .map_err(|_catalog| ProjectionError::Unavailable)?
                    != *name
                {
                    return Err(ProjectionError::Unavailable);
                }
                Ok(wire::TokenChoice {
                    rules_name: name.clone(),
                    card: Some(card),
                })
            })
            .collect::<Result<_>>()?;
        let id = option(context.id, index);
        coordinates.insert(
            id.clone(),
            u64::try_from(index).map_err(|_range| ProjectionError::Range)?,
        );
        options.push(wire::ChoiceOption {
            id,
            value: wire::ChoiceValue::Tokens { tokens },
            cost: None,
        });
    }
    Ok((
        wire::QuestionUse::TokenCapacity,
        wire::QuestionShape::Choose {
            options,
            bounds: wire::Bounds {
                min: Nat::from(1_u32),
                max: Some(Nat::from(1_u32)),
            },
            count: None,
            earlier: vec![],
            capacity: Some(wire::Capacity {
                destination: wire::Destination {
                    player: seat(domain.destination),
                    zone: zone(domain.zone),
                    position: None,
                },
                available,
            }),
        },
        super::Binding::Capacity(Binding::Tokens(coordinates)),
    ))
}

pub(super) fn movement(
    domain: &MovementCapacityQuestion,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let facts = domain.facts.as_ref().ok_or(ProjectionError::Unavailable)?;
    if context.observation.viewer != seat(context.actor)
        || facts.len() != domain.selection.cards.len()
    {
        return Err(ProjectionError::Unavailable);
    }
    let projection = SelectionProjection::capacity(context.id, domain, |coordinate| {
        let option = domain
            .selection
            .cards
            .iter()
            .find(|card| card.handle == coordinate)
            .ok_or(ProjectionError::Unavailable)?;
        details(
            option,
            facts.get(&coordinate).ok_or(ProjectionError::Unavailable)?,
            context,
        )
    })?;
    Ok((
        wire::QuestionUse::MovementCapacity,
        wire::QuestionShape::Select {
            selection: projection.selection().clone(),
        },
        super::Binding::Capacity(Binding::Movement(Box::new(projection))),
    ))
}

fn details(
    option: &CardOption,
    facts: &CapacityCard,
    context: Context<'_>,
) -> Result<CandidateDetails> {
    let face = match facts {
        CapacityCard::Existing { face, .. } | CapacityCard::Token { face } => face,
    };
    if face.card != option.card {
        return Err(ProjectionError::Unavailable);
    }
    let card = match facts {
        CapacityCard::Token { .. } => created_card(face, context)?,
        CapacityCard::Existing { .. } => context.references.resolve(context.catalog, face)?,
    };
    if let Some(handle) = &option.object {
        let CapacityCard::Existing {
            controller: Some(controller),
            ..
        } = facts
        else {
            return Err(ProjectionError::Unavailable);
        };
        let member = SelectionMember {
            handle: option.handle,
            player: *controller,
            card: Some(option.card.clone()),
            name: Some(option.name.clone()),
            object: Some(handle.clone()),
            rules_names: option.rules_names.clone(),
            original_cost: option.original_cost,
        };
        let linked = super::candidates::details(&member, context)?;
        let Some(Subject::Card {
            object: Some(object),
            ..
        }) = &linked.subject
        else {
            return Err(ProjectionError::Unavailable);
        };
        if linked.controller != Some(seat(*controller))
            || context
                .observation
                .objects
                .get(object)
                .is_none_or(|value| value.card != card)
        {
            return Err(ProjectionError::Unavailable);
        }
        // The physical printing stays frozen; the same board supplies any effective mask or evolve face.
        return Ok(linked);
    }
    if matches!(
        facts,
        CapacityCard::Existing {
            controller: Some(_),
            ..
        }
    ) {
        return Err(ProjectionError::Unavailable);
    }
    // A printing-only candidate grants no current object, controller or location capability.
    Ok(CandidateDetails {
        subject: Some(Subject::Card { card, object: None }),
        controller: None,
        origin: None,
        classes: None,
    })
}

fn created_card(face: &CardFace, context: Context<'_>) -> Result<CardRef> {
    let card = context.references.resolve(context.catalog, face)?;
    // The catalog classifies this exact printing; its display label is not a lookup key.
    if !context.catalog.token_definitions.contains_key(&face.card) {
        return Err(ProjectionError::Unavailable);
    }
    Ok(card)
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        match self {
            Self::Movement(projection) => projection
                .capacity_answer(answer)
                .map_err(|_projection| AnswerError),
            Self::Tokens(coordinates) => {
                let Answer::Choose { chosen } = answer else {
                    return Err(AnswerError);
                };
                let [id] = chosen.as_slice() else {
                    return Err(AnswerError);
                };
                coordinates
                    .get(id)
                    .copied()
                    .map(player::Answer::TokenCapacity)
                    .ok_or(AnswerError)
            }
        }
    }
}
