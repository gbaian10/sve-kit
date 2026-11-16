use super::common::{option, seat};
use super::decision;
use super::{AnswerError, Binding, Result};
use crate::battle::{BattleReferences, CandidateDetails, ProjectionError, SelectionProjection};
use crate::catalog::Catalog;
use crate::game::player::{self, EvolutionAnswer, Viewer, observation::CardFace};
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{
    Answer, Int, Nat, QuestionId, Subject, Zone, observation::BattleObservation, question as wire,
};

pub(super) fn evolution_form(
    id: &QuestionId,
    actor: Viewer,
    forms: &[player::EvolutionForm],
    observation: &BattleObservation,
    catalog: &Catalog,
    references: &BattleReferences,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    if forms.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    let deck = &observation
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .ok_or(ProjectionError::Unavailable)?
        .zones
        .evolve_deck;
    let mut seen = BTreeSet::new();
    let mut choices = Vec::new();
    let mut coordinates = BTreeMap::new();
    for (index, form) in forms.iter().enumerate() {
        if !seen.insert((&form.card.handle, form.face)) {
            return Err(ProjectionError::Unavailable);
        }
        let member = deck
            .cards
            .iter()
            .find(|member| member.object.as_str() == form.card.handle.as_str())
            .ok_or(ProjectionError::Unavailable)?;
        let object = observation
            .objects
            .get(&member.object)
            .ok_or(ProjectionError::Unavailable)?;
        // The native form authorizes a destination, which may differ from its observed current face.
        let destination = references.resolve(
            catalog,
            &CardFace {
                card: form.card.properties.card.clone(),
                face: form.face,
            },
        )?;
        if destination.printing_id != object.card.printing_id {
            return Err(ProjectionError::Unavailable);
        }
        let option_id = option(id, index);
        // The destination CardRef lets clients read its name from the pinned snapshot.
        choices.push(wire::ChoiceOption {
            id: option_id.clone(),
            value: wire::ChoiceValue::EvolutionForm {
                object: member.object.clone(),
                face: destination,
            },
            cost: None,
        });
        coordinates.insert(option_id, form.id);
    }
    Ok((
        wire::QuestionUse::EvolutionForm,
        wire::QuestionShape::Choose {
            options: choices,
            bounds: wire::Bounds {
                min: Nat::from(1_u32),
                max: Some(Nat::from(1_u32)),
            },
            count: None,
            earlier: vec![],
            capacity: None,
        },
        Binding::EvolutionForm(coordinates),
    ))
}

pub(super) fn evolution_payment(
    quote: player::EvolutionPayment,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    if quote.total_pp < 0 || quote.available_pp < 0 {
        return Err(ProjectionError::Unavailable);
    }
    let mut payments = Vec::new();
    // One optional EP replaces exactly one PP; changing that rule also changes this candidate template.
    for use_ep in [false, true] {
        let pp = quote
            .total_pp
            .checked_sub(i64::from(use_ep))
            .ok_or(ProjectionError::Unavailable)?;
        for super_evolution in [false, true] {
            if !quote.is_valid(pp, use_ep, super_evolution) {
                continue;
            }
            payments.push(wire::EvolutionPayment {
                pp: Int::try_from(pp)?,
                use_ep,
                super_evolution,
            });
        }
    }
    if payments.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    Ok((
        wire::QuestionUse::EvolutionPayment,
        wire::QuestionShape::EvolutionPayment {
            payments: payments.clone(),
        },
        Binding::EvolutionPayment(payments),
    ))
}

pub(super) fn evolution_payment_answer(
    payments: &[wire::EvolutionPayment],
    answer: &Answer,
) -> result::Result<EvolutionAnswer, AnswerError> {
    let Answer::EvolutionPayment {
        pp,
        use_ep,
        super_evolution,
    } = answer
    else {
        return Err(AnswerError);
    };
    let chosen = wire::EvolutionPayment {
        pp: *pp,
        use_ep: *use_ep,
        super_evolution: *super_evolution,
    };
    if !payments.contains(&chosen) {
        return Err(AnswerError);
    }
    Ok(EvolutionAnswer::Payment {
        pp: pp.get(),
        ep: *use_ep,
        super_evolution: *super_evolution,
    })
}

pub(super) fn materials(
    domain: &player::SelectionQuestion,
    context: decision::Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    // The native material answer has no role channel and cannot name leaders.
    if !domain.roles.is_empty()
        || domain.members.iter().any(|member| member.card.is_none())
        || u64::try_from(domain.members.len())
            .ok()
            .is_none_or(|count| domain.cardinality.minimum.0 > count)
    {
        return Err(ProjectionError::Unavailable);
    }
    let projection =
        SelectionProjection::new(context.id, domain, &BTreeMap::new(), |coordinate| {
            let member = domain
                .members
                .iter()
                .find(|member| member.handle == coordinate)
                .ok_or(ProjectionError::Unavailable)?;
            decision::details(member, context)
        })?;
    Ok((
        wire::QuestionUse::EvolutionMaterial,
        wire::QuestionShape::Select {
            selection: projection.selection().clone(),
        },
        Binding::EvolutionMaterials(projection),
    ))
}

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
    selection_answer(projection, answer).map(EvolutionAnswer::Discard)
}

pub(super) fn materials_answer(
    projection: &SelectionProjection,
    answer: &Answer,
) -> result::Result<EvolutionAnswer, AnswerError> {
    selection_answer(projection, answer).map(EvolutionAnswer::Materials)
}

fn selection_answer(
    projection: &SelectionProjection,
    answer: &Answer,
) -> result::Result<Vec<u64>, AnswerError> {
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
    Ok(handles)
}

#[cfg(test)]
mod tests;
