use super::{AnswerError, Context, Result};
use crate::battle::{ProjectionError, SelectionProjection};
use crate::game::player::{self, SelectionQuestion};
use alloc::boxed::Box;
use core::result;
use sve_battle_contract::{Answer, question as wire};

#[derive(Clone)]
pub(in crate::battle::question) struct Binding {
    materials: SelectionProjection,
    ep_allowed: bool,
}

pub(super) fn project(
    domain: &SelectionQuestion,
    ep_allowed: bool,
    context: Context<'_>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, super::Binding)> {
    if context.purpose != player::QuestionPurpose::PlayParameter || !domain.roles.is_empty() {
        return Err(ProjectionError::Unavailable);
    }
    let materials = super::select::freeze(domain, context)?;
    Ok((
        wire::QuestionUse::ResourceEntryPayment,
        wire::QuestionShape::ResourcePayment {
            materials: materials.selection().clone(),
            ep_allowed,
        },
        super::Binding::Resource(Box::new(Binding {
            materials,
            ep_allowed,
        })),
    ))
}

impl Binding {
    pub(super) fn answer(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        let Answer::ResourcePayment { materials, use_ep } = answer else {
            return Err(AnswerError);
        };
        if *use_ep && !self.ep_allowed {
            return Err(AnswerError);
        }
        let count = u64::try_from(materials.len()).map_err(|_range| AnswerError)?;
        let bounds = self.materials.selection().bounds;
        if count < bounds.min.get() || bounds.max.is_some_and(|max| count > max.get()) {
            return Err(AnswerError);
        }
        let player::Answer::Targets(handles) = self
            .materials
            .targets(&Answer::Select {
                selected: materials.clone(),
                groups: None,
                roles: vec![],
            })
            .map_err(|_projection| AnswerError)?
        else {
            return Err(AnswerError);
        };
        // EP permission preserves the native input domain; affordability is rechecked on submission.
        Ok(player::Answer::ResourceEntryPayment {
            materials: handles,
            use_ep: *use_ep,
        })
    }
}
