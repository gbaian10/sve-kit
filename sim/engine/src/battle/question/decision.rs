//! Effect domains retain their original native answer channels.

use super::{AnswerError, Result};
use crate::battle::ProjectionError;
use crate::game::player::{self, DecisionShape, QuestionToken};
use core::result;
use sve_battle_contract::{Answer, question as wire};

mod confirm;
mod number;

#[derive(Clone)]
pub(super) enum Binding {
    Confirm(confirm::Binding),
    Number(number::Binding),
}

pub(super) fn project(
    domain: &DecisionShape,
) -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    match domain {
        DecisionShape::OptionalCost => Ok(confirm::project(confirm::Binding::OptionalCost)),
        DecisionShape::ResolutionPayment => {
            Ok(confirm::project(confirm::Binding::ResolutionPayment))
        }
        DecisionShape::May => Ok(confirm::project(confirm::Binding::May)),
        DecisionShape::Integer(domain) => number::integer(*domain),
        DecisionShape::DiceReroll { value, remaining } => number::dice(*value, *remaining),
        DecisionShape::Abilities(_)
        | DecisionShape::ResourceEntryPayment { .. }
        | DecisionShape::WardEntry { .. }
        | DecisionShape::Quick { .. }
        | DecisionShape::CostMaterials(_)
        | DecisionShape::Discard { .. }
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
        | DecisionShape::Targets(_)
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
        }
    }
}

#[cfg(test)]
mod tests;
