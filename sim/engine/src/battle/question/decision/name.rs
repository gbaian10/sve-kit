use super::{AnswerError, Context, Result};
use crate::battle::ProjectionError;
use crate::battle::question::common::{option, seat};
use crate::game::player;
use alloc::collections::BTreeMap;
use core::result;
use sve_battle_contract::{Answer, Nat, OptionId, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding(BTreeMap<OptionId, String>);

pub(super) fn project(
    names: &[String],
    context: Option<Context<'_>>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    let context = context.ok_or(ProjectionError::Unavailable)?;
    if names.is_empty() || context.observation.viewer != seat(context.actor) {
        return Err(ProjectionError::Unavailable);
    }
    let mut coordinates = BTreeMap::new();
    let options = names
        .iter()
        .enumerate()
        .map(|(index, rules_name)| {
            let id = option(context.id, index);
            coordinates.insert(id.clone(), rules_name.clone());
            wire::ChoiceOption {
                id,
                value: wire::ChoiceValue::CardName {
                    rules_name: rules_name.clone(),
                },
                cost: None,
            }
        })
        .collect();
    let one = Nat::from(1_u32);
    Ok((
        wire::QuestionUse::DeclareName,
        wire::QuestionShape::Choose {
            options,
            bounds: wire::Bounds {
                min: one,
                max: Some(one),
            },
            count: Some(one),
            earlier: vec![],
            capacity: None,
        },
        super::Binding::Name(Binding(coordinates)),
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::Choose { chosen } = answer else {
            return Err(AnswerError);
        };
        let [id] = chosen.as_slice() else {
            return Err(AnswerError);
        };
        self.0
            .get(id)
            .cloned()
            .map(player::Answer::DeclareName)
            .ok_or(AnswerError)
    }
}
