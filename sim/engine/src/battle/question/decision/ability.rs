use super::{AnswerError, Context, Result, text};
use crate::battle::ProjectionError;
use crate::battle::question::common::{option, seat};
use crate::game::player::{self, AbilityChoice};
use alloc::collections::{BTreeMap, BTreeSet};
use core::result;
use sve_battle_contract::{Answer, Nat, OptionId, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    coordinates: BTreeMap<OptionId, u64>,
}

pub(super) fn project(
    choices: &[AbilityChoice],
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    if choices.is_empty() || context.observation.viewer != seat(context.actor) {
        return Err(ProjectionError::Unavailable);
    }
    let mut coordinates = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut options = vec![];
    for (index, choice) in choices.iter().enumerate() {
        if !seen.insert(choice.handle) {
            return Err(ProjectionError::Unavailable);
        }
        let source = text::project(
            choice.source.as_ref().ok_or(ProjectionError::Unavailable)?,
            context,
        )?;
        let id = option(context.id, index);
        coordinates.insert(id.clone(), choice.handle);
        options.push(wire::ChoiceOption {
            id,
            value: wire::ChoiceValue::Ability { source },
            cost: None,
        });
    }
    Ok((
        wire::QuestionUse::Ability,
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
        super::Binding::Ability(Binding { coordinates }),
    ))
}
impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Choose { chosen } = answer else {
            return Err(AnswerError);
        };
        let [choice] = chosen.as_slice() else {
            return Err(AnswerError);
        };
        Ok(player::Answer::Ability(
            *self.coordinates.get(choice).ok_or(AnswerError)?,
        ))
    }
}
