use super::{AnswerError, Context, ProjectionError, Result};
use crate::battle::{question::common, sources};
use crate::game::player::{self, PendingChoice};
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{Answer, Nat, ObjectHandle, OptionId, question as wire};

mod event;

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    coordinates: BTreeMap<OptionId, u64>,
}

pub(super) fn project(
    choices: &[PendingChoice],
    context: Context<'_>,
) -> Result<(wire::QuestionShape, Binding)> {
    let (options, binding) = options(choices, context)?;
    Ok((shape(options, vec![]), binding))
}

pub(super) fn options(
    choices: &[PendingChoice],
    context: Context<'_>,
) -> Result<(Vec<wire::ChoiceOption>, Binding)> {
    if choices.is_empty()
        || context.purpose != player::QuestionPurpose::RuleChoice
        || context.observation.viewer != common::seat(context.actor)
    {
        return Err(ProjectionError::Unavailable);
    }
    let mut coordinates = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut options = vec![];
    for (index, choice) in choices.iter().enumerate() {
        if !seen.insert(choice.handle) {
            return Err(ProjectionError::Unavailable);
        }
        let id = common::option(context.id, index);
        options.push(wire::ChoiceOption {
            id: id.clone(),
            value: value(choice, context.actor, context)?,
            cost: None,
        });
        coordinates.insert(id, choice.handle);
    }
    Ok((options, Binding { coordinates }))
}

pub(super) fn shape(
    options: Vec<wire::ChoiceOption>,
    earlier: Vec<wire::EarlierChoice>,
) -> wire::QuestionShape {
    wire::QuestionShape::Choose {
        options,
        bounds: wire::Bounds {
            min: Nat::from(1_u32),
            max: Some(Nat::from(1_u32)),
        },
        count: None,
        earlier,
        capacity: None,
    }
}

// Earlier quota choices may belong to a different controller than this question's actor.
pub(super) fn value(
    choice: &PendingChoice,
    controller: player::Viewer,
    context: Context<'_>,
) -> Result<wire::ChoiceValue> {
    let source = choice.source.as_ref().ok_or(ProjectionError::Unavailable)?;
    let holder = source
        .holder
        .as_ref()
        .map(|handle| {
            let key = ObjectHandle::new(handle.as_str().into());
            if !context.observation.objects.contains_key(&key) {
                return Err(ProjectionError::Unavailable);
            }
            Ok(key)
        })
        .transpose()?;
    let occurrence = choice
        .occurrence
        .as_ref()
        .ok_or(ProjectionError::Unavailable)?;
    Ok(wire::ChoiceValue::Occurrence {
        source: sources::source_ref(context.catalog, context.references, source, holder)?,
        controller: common::seat(controller),
        event: event::project(&occurrence.event)?,
        position: occurrence.position.map(position).transpose()?,
    })
}

fn position(native: u64) -> Result<Nat> {
    let offset = native.checked_sub(1).ok_or(ProjectionError::Unavailable)?;
    Nat::try_from(offset).map_err(|_range| ProjectionError::Unavailable)
}

impl Binding {
    pub(super) fn chosen(&self, answer: &Answer) -> result::Result<u64, AnswerError> {
        let Answer::Choose { chosen } = answer else {
            return Err(AnswerError);
        };
        let [choice] = chosen.as_slice() else {
            return Err(AnswerError);
        };
        self.coordinates.get(choice).copied().ok_or(AnswerError)
    }

    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        Ok(player::Answer::Pending(self.chosen(answer)?))
    }
}

#[cfg(test)]
mod tests;
