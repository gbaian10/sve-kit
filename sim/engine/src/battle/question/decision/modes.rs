use super::{AnswerError, Context, Result};
use crate::battle::ProjectionError;
use crate::battle::question::common::{option, seat};
use crate::battle::selection::bounds;
use crate::game::player;
use crate::ir::{Cardinality, ModeId, Nat};
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{Answer, Nat as WireNat, OptionId, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    coordinates: BTreeMap<OptionId, ModeId>,
    bounds: wire::Bounds,
}

pub(super) fn project(
    modes: &[ModeId],
    cardinality: Cardinality,
    count: Nat,
    usage: wire::QuestionUse,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let mut effective = bounds(cardinality)?;
    if usage == wire::QuestionUse::ResolutionAlternative
        && (cardinality.minimum != Nat(1) || cardinality.maximum != Some(Nat(1)) || count != Nat(1))
    {
        return Err(ProjectionError::Unavailable);
    }
    let count = WireNat::try_from(count.0)?;
    let available = WireNat::try_from(modes.len())?;
    let cap = count.min(available);
    if context.observation.viewer != seat(context.actor)
        || count == WireNat::ZERO
        || (effective.min == WireNat::ZERO && cap != available)
    {
        return Err(ProjectionError::Unavailable);
    }
    effective.max = Some(effective.max.map_or(cap, |max| max.min(cap)));
    if effective.max.is_some_and(|max| effective.min > max) {
        return Err(ProjectionError::Unavailable);
    }
    let mut seen = BTreeSet::new();
    let mut coordinates = BTreeMap::new();
    let mut options = vec![];
    for (index, mode) in modes.iter().enumerate() {
        if !seen.insert(*mode) {
            return Err(ProjectionError::Unavailable);
        }
        let value = WireNat::try_from(mode.0)?;
        let id = option(context.id, index);
        coordinates.insert(id.clone(), *mode);
        options.push(wire::ChoiceOption {
            id,
            value: wire::ChoiceValue::Mode {
                mode: value,
                // The producer has no exact per-mode wording span.
                text: None,
            },
            cost: None,
        });
    }
    Ok((
        usage,
        wire::QuestionShape::Choose {
            options,
            bounds: effective,
            count: Some(count),
            earlier: vec![],
            capacity: None,
        },
        super::Binding::Modes(Binding {
            coordinates,
            bounds: effective,
        }),
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Choose { chosen } = answer else {
            return Err(AnswerError);
        };
        let size = u64::try_from(chosen.len()).map_err(|_range| AnswerError)?;
        if size < self.bounds.min.get() || self.bounds.max.is_some_and(|max| size > max.get()) {
            return Err(AnswerError);
        }
        let mut seen = BTreeSet::new();
        let selected = chosen
            .iter()
            .map(|id| {
                if !seen.insert(id) {
                    return Err(AnswerError);
                }
                self.coordinates.get(id).copied().ok_or(AnswerError)
            })
            .collect::<result::Result<_, _>>()?;
        // Native validation still checks the original mode decision and evaluated count.
        Ok(player::Answer::Modes(selected))
    }
}
