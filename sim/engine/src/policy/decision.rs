use super::{PolicyFailure, distribute, search, select};
use crate::game::player::{Answer, CardOption, DecisionShape, DiscardKind, SelectionQuestion};
use crate::ir::{Cardinality, ModeId, Nat, SearchReplacementAnswer, StepwiseAnswer, TriggerIcon};
use alloc::collections::BTreeSet;

pub(super) fn ordered(handles: impl Iterator<Item = u64>) -> Result<Vec<u64>, PolicyFailure> {
    let handles: Vec<_> = handles.collect();
    if handles.iter().collect::<BTreeSet<_>>().len() != handles.len() {
        return Err(PolicyFailure::InvalidQuestion);
    }
    Ok(handles)
}

pub(super) fn count(bounds: Cardinality, available: usize) -> Result<usize, PolicyFailure> {
    if bounds
        .maximum
        .is_some_and(|maximum| maximum.0 < bounds.minimum.0)
    {
        return Err(PolicyFailure::InvalidQuestion);
    }
    let minimum = usize::try_from(bounds.minimum.0).map_err(|_error| PolicyFailure::NoSolution)?;
    let maximum = bounds.maximum.map_or(available, |maximum| {
        usize::try_from(maximum.0)
            .unwrap_or(usize::MAX)
            .min(available)
    });
    if minimum > maximum {
        return Err(PolicyFailure::NoSolution);
    }
    Ok(if maximum == 0 { 0 } else { minimum.max(1) })
}

fn partition(cards: &[CardOption], bounds: Cardinality) -> Result<Vec<u64>, PolicyFailure> {
    let handles = ordered(cards.iter().map(|card| card.handle))?;
    let count = count(bounds, handles.len())?;
    Ok(handles.into_iter().take(count).collect())
}

pub(super) fn materials(question: &SelectionQuestion) -> Result<Vec<u64>, PolicyFailure> {
    if !question.roles.is_empty() {
        return Err(PolicyFailure::InvalidQuestion);
    }
    Ok(select(question)?.targets)
}

fn modes(ids: &[ModeId], bounds: Cardinality, limit: Nat) -> Result<Vec<ModeId>, PolicyFailure> {
    drop(ordered(ids.iter().map(|id| id.0))?);
    let cap = usize::try_from(limit.0)
        .unwrap_or(usize::MAX)
        .min(ids.len());
    // Native zero availability skips the printed minimum, rather than inventing an option.
    if cap == 0 {
        return Ok(Vec::new());
    }
    if bounds.minimum == Nat(0) && cap != ids.len() {
        return Err(PolicyFailure::NoSolution);
    }
    let count = count(bounds, cap)?;
    Ok(ids.iter().take(count).copied().collect())
}

#[expect(
    clippy::too_many_lines,
    reason = "Exhaustive public decision mapping keeps every native answer channel visible."
)]
pub(super) fn answer(question: &DecisionShape) -> Result<Answer, PolicyFailure> {
    Ok(match question {
        DecisionShape::DiceReroll { .. } => Answer::Integer(Nat(0)),
        DecisionShape::Abilities(options) => {
            Answer::Ability(options.first().ok_or(PolicyFailure::NoSolution)?.handle)
        }
        DecisionShape::ResourceEntryPayment {
            materials: question,
            ep_allowed,
        } => Answer::ResourceEntryPayment {
            materials: materials(question)?,
            use_ep: *ep_allowed,
        },
        DecisionShape::WardEntry { .. } => Answer::WardEntry(true),
        DecisionShape::Quick { .. } => Answer::Quick(None),
        DecisionShape::OptionalCost | DecisionShape::ResolutionPayment => {
            Answer::OptionalCost(true)
        }
        DecisionShape::May => Answer::Integer(Nat(1)),
        DecisionShape::BeforeDraw => Answer::Stepwise(StepwiseAnswer::Stop),
        DecisionShape::CostMaterials(question)
        | DecisionShape::Discard {
            kind: DiscardKind::Cost,
            cards: question,
        } => Answer::CostMaterials(materials(question)?),
        DecisionShape::Discard {
            kind: DiscardKind::Effect,
            cards,
        } => Answer::Targets(materials(cards)?),
        DecisionShape::ExchangeRoutes(routes) => {
            Answer::Integer(*routes.first().ok_or(PolicyFailure::NoSolution)?)
        }
        DecisionShape::Pending(options) => {
            Answer::Pending(options.first().ok_or(PolicyFailure::NoSolution)?.handle)
        }
        DecisionShape::TriggerQuota(quota) => Answer::TriggerQuota(
            quota
                .candidates
                .first()
                .ok_or(PolicyFailure::NoSolution)?
                .handle,
        ),
        DecisionShape::Integer(bounds) => {
            if bounds
                .maximum
                .is_some_and(|maximum| maximum.0 < bounds.minimum.0)
            {
                return Err(PolicyFailure::InvalidQuestion);
            }
            Answer::Integer(bounds.minimum)
        }
        DecisionShape::Order(cards) => {
            Answer::Order(ordered(cards.iter().map(|card| card.handle))?)
        }
        DecisionShape::DamageReplacements(options) => {
            Answer::Order(ordered(options.iter().map(|option| option.handle))?)
        }
        DecisionShape::DriveTrigger { icon, targets } => Answer::DriveTrigger {
            accept: !matches!(icon, TriggerIcon::Front),
            target: match icon {
                TriggerIcon::Critical | TriggerIcon::Stand => {
                    targets.first().map(|card| card.handle)
                }
                TriggerIcon::Draw | TriggerIcon::Heal | TriggerIcon::Front => None,
            },
        },
        DecisionShape::Search(question) => Answer::Search(search(question)?),
        DecisionShape::SearchPartition(question) => {
            Answer::SearchPartition(partition(&question.cards, question.cardinality)?)
        }
        DecisionShape::MovementCapacity(question) => Answer::SearchPartition(partition(
            &question.selection.cards,
            question.selection.cardinality,
        )?),
        DecisionShape::Distribution(question) => Answer::Distribution(distribute(question)?),
        DecisionShape::SearchReplacement => {
            Answer::SearchReplacement(SearchReplacementAnswer::Original)
        }
        DecisionShape::Targets(question) => {
            let selected = select(question)?;
            if question.roles.is_empty() {
                Answer::Targets(selected.targets)
            } else {
                Answer::TargetsWithRoles {
                    targets: selected.targets,
                    roles: selected.roles,
                }
            }
        }
        DecisionShape::TokenCapacity(question) => {
            question.choices.first().ok_or(PolicyFailure::NoSolution)?;
            Answer::TokenCapacity(0)
        }
        DecisionShape::Modes {
            modes: ids,
            cardinality,
            count,
        }
        | DecisionShape::ResolutionAlternative {
            modes: ids,
            cardinality,
            count,
        } => Answer::Modes(modes(ids, *cardinality, *count)?),
    })
}
