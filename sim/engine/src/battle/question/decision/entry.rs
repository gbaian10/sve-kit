use super::{AnswerError, Context, Result, candidates};
use crate::battle::ProjectionError;
use crate::battle::question::common::{option, seat};
use crate::game::player::{self, DecisionShape, SelectionMember};
use crate::ir::StepwiseAnswer;
use core::result;
use sve_battle_contract::{Answer, Nat, OptionId, Subject, Zone, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) enum Binding {
    Draw { go: OptionId, stop: OptionId },
    Ward,
}

pub(super) fn draw(
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    if context.observation.viewer != seat(context.actor) {
        return Err(ProjectionError::Unavailable);
    }
    let go = option(context.id, 0);
    let stop = option(context.id, 1);
    let options = [
        (go.clone(), wire::StepwiseChoice::Continue),
        (stop.clone(), wire::StepwiseChoice::Stop),
    ]
    .into_iter()
    .map(|(id, step)| wire::ChoiceOption {
        id,
        value: wire::ChoiceValue::Stepwise { step },
        cost: None,
    })
    .collect();
    Ok((
        wire::QuestionUse::DrawContinue,
        wire::QuestionShape::Choose {
            options,
            bounds: wire::Bounds {
                min: Nat::from(1_u32),
                max: Some(Nat::from(1_u32)),
            },
            count: None,
            earlier: vec![],
            capacity: None,
        },
        super::Binding::Entry(Binding::Draw { go, stop }),
    ))
}

pub(super) fn ward(
    domain: &DecisionShape,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let DecisionShape::WardEntry {
        card,
        name,
        object,
        zone,
        position,
        face,
    } = domain
    else {
        return Err(ProjectionError::Unavailable);
    };
    if context.observation.viewer != seat(context.actor) {
        return Err(ProjectionError::Unavailable);
    }
    if face.card != *card {
        return Err(ProjectionError::Unavailable);
    }
    let source = context.references.resolve(context.catalog, face)?;
    if zone == "void" {
        // The native sentinel is 1 before a token has any source zone membership.
        if object.is_some() || *position != 1 {
            return Err(ProjectionError::Unavailable);
        }
        return Ok((
            wire::QuestionUse::WardEntry,
            wire::QuestionShape::Confirm {
                allowed: vec![false, true],
                subject: Some(Subject::Card {
                    card: source,
                    object: None,
                }),
                // A newly created token has no source membership in a rules zone.
                origin: None,
            },
            super::Binding::Entry(Binding::Ward),
        ));
    }
    let facts = candidates::details(
        &SelectionMember {
            handle: 0,
            player: context.actor,
            card: Some(card.clone()),
            name: Some(name.clone()),
            object: object.clone(),
            rules_names: None,
            original_cost: None,
        },
        context,
    )?;
    let origin = facts.origin.ok_or(ProjectionError::Unavailable)?;
    let expected_zone = match zone.as_str() {
        "deck" => Zone::Deck,
        "evolve_deck" => Zone::EvolveDeck,
        "hand" => Zone::Hand,
        "field" => Zone::Field,
        "ex" => Zone::Ex,
        "cemetery" => Zone::Cemetery,
        "banish" => Zone::Banish,
        "resolution" => Zone::Resolution,
        "evolution" => Zone::Evolution,
        "race" => Zone::Race,
        "drive" => Zone::Drive,
        "trigger" => Zone::Trigger,
        "equipment" => Zone::Equipment,
        _ => return Err(ProjectionError::Unavailable),
    };
    // Changed effective faces need an explicit entry display policy before projection.
    if facts.controller != Some(seat(context.actor))
        || origin.zone != expected_zone
        || origin.position
            != Some(Nat::try_from(
                position
                    .checked_sub(1)
                    .ok_or(ProjectionError::Unavailable)?,
            )?)
        || !matches!(&facts.subject, Some(Subject::Card { card: resolved, .. }) if *resolved == source)
    {
        return Err(ProjectionError::Unavailable);
    }
    Ok((
        wire::QuestionUse::WardEntry,
        wire::QuestionShape::Confirm {
            // The native entry procedure offers both postures and revalidates Ward on submission.
            allowed: vec![false, true],
            subject: facts.subject,
            origin: Some(origin),
        },
        super::Binding::Entry(Binding::Ward),
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        match self {
            Self::Ward => {
                let Answer::Confirm { accept } = answer else {
                    return Err(AnswerError);
                };
                Ok(player::Answer::WardEntry(*accept))
            }
            Self::Draw { go, stop } => {
                let Answer::Choose { chosen } = answer else {
                    return Err(AnswerError);
                };
                let [choice] = chosen.as_slice() else {
                    return Err(AnswerError);
                };
                let step = if choice == go {
                    StepwiseAnswer::Continue
                } else if choice == stop {
                    StepwiseAnswer::Stop
                } else {
                    return Err(AnswerError);
                };
                Ok(player::Answer::Stepwise(step))
            }
        }
    }
}
