use super::{Context, Result, order, text};
use crate::battle::ProjectionError;
use crate::battle::question::common::{option, seat};
use crate::game::player::ReplacementChoice;
use alloc::collections::{BTreeMap, BTreeSet};
use sve_battle_contract::question as wire;

pub(super) fn project(
    choices: &[ReplacementChoice],
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    if choices.len() < 2 || context.observation.viewer != seat(context.actor) {
        return Err(ProjectionError::Unavailable);
    }
    let mut coordinates = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut items = vec![];
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
        items.push(wire::OrderItem {
            id,
            subject: None,
            source: Some(source),
        });
    }
    Ok((
        wire::QuestionUse::DamageReplacementOrder,
        wire::QuestionShape::Order {
            items,
            direction: wire::OrderDirection::Application,
            destination: None,
        },
        super::Binding::Order(order::Binding::new(coordinates)),
    ))
}
