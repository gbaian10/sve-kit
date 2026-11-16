//! Effect domains retain their original native answer channels.

use super::{AnswerError, Result};
use crate::battle::ProjectionError;
use crate::game::player::{self, DecisionShape, QuestionToken, Viewer};
use alloc::boxed::Box;
use core::result;
use sve_battle_contract::{Answer, QuestionId, observation::BattleObservation, question as wire};

mod confirm;
mod number;
mod select;

#[derive(Clone, Copy)]
pub(super) struct Context<'projection> {
    pub(super) id: &'projection QuestionId,
    pub(super) actor: Viewer,
    pub(super) purpose: player::QuestionPurpose,
    pub(super) observation: &'projection BattleObservation,
}

#[derive(Clone)]
pub(super) enum Binding {
    Confirm(confirm::Binding),
    Number(number::Binding),
    Select(Box<select::Binding>),
}

pub(super) fn project(
    domain: &DecisionShape,
    context: Option<Context<'_>>,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    match domain {
        DecisionShape::OptionalCost => Ok(confirm::project(confirm::Binding::OptionalCost)),
        DecisionShape::ResolutionPayment => {
            Ok(confirm::project(confirm::Binding::ResolutionPayment))
        }
        DecisionShape::May => Ok(confirm::project(confirm::Binding::May)),
        DecisionShape::Integer { bounds, source } => number::integer(*bounds, *source),
        DecisionShape::DiceReroll { value, remaining } => number::dice(*value, *remaining),
        DecisionShape::Targets(domain) => select::project(
            domain,
            select::Kind::Targets,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::Discard {
            kind: player::DiscardKind::Cost,
            cards,
        } => select::project(
            cards,
            select::Kind::CostDiscard,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::Discard {
            kind: player::DiscardKind::Effect,
            cards,
        } => select::project(
            cards,
            select::Kind::EffectDiscard,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::CostMaterials(domain) => select::project(
            domain,
            select::Kind::CostMaterial,
            context.ok_or(ProjectionError::Unavailable)?,
        ),
        DecisionShape::Abilities(_)
        | DecisionShape::ResourceEntryPayment { .. }
        | DecisionShape::WardEntry { .. }
        | DecisionShape::Quick { .. }
        | DecisionShape::ExchangeRoutes(_)
        | DecisionShape::BeforeDraw
        | DecisionShape::Pending(_)
        | DecisionShape::TriggerQuota(_)
        | DecisionShape::Order(_)
        | DecisionShape::DamageReplacements(_)
        | DecisionShape::DriveTrigger { .. }
        | DecisionShape::Search(_)
        | DecisionShape::SearchPartition(_)
        | DecisionShape::MovementCapacity(_)
        | DecisionShape::Distribution(_)
        | DecisionShape::SearchReplacement
        | DecisionShape::TokenCapacity(_)
        | DecisionShape::Modes { .. }
        | DecisionShape::ResolutionAlternative { .. } => Err(ProjectionError::Unavailable),
    }
}

impl Binding {
    pub(super) fn answer(
        &self,
        token: QuestionToken,
        answer: &Answer,
    ) -> result::Result<player::Command, AnswerError> {
        Ok(player::Command::Answer {
            token,
            answer: self.decode(answer)?,
        })
    }

    fn decode(&self, answer: &Answer) -> result::Result<player::Answer, AnswerError> {
        match self {
            Self::Confirm(binding) => binding.answer(answer),
            Self::Number(binding) => binding.answer(answer),
            Self::Select(binding) => binding.answer(answer),
        }
    }
}

#[cfg(test)]
mod tests;
