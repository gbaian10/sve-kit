//! Rule-owned decisions are separate from effect answer domains.
//! F02A owns this family; F02B/F02D keep effect shapes in Decision.

use super::{AnswerError, Binding as QuestionBinding, Result, decision::Context};
use crate::battle::{BattleReferences, ProjectionError};
use crate::catalog::Catalog;
use crate::game::player::{self, DecisionShape, QuestionToken};
use core::result;
use sve_battle_contract::{
    Answer,
    observation::{BattleObservation, ProcessingStage},
    question as wire,
};

mod pending;
mod quick;

#[derive(Clone)]
pub(super) enum Binding {
    Quick(quick::Binding),
    Pending(pending::Binding),
}

pub(super) fn context(
    purpose: player::QuestionPurpose,
    observation: &BattleObservation,
) -> Result<(
    player::QuestionPurpose,
    wire::QuestionPurpose,
    ProcessingStage,
)> {
    let wire_purpose = match purpose {
        player::QuestionPurpose::PlayParameter => wire::QuestionPurpose::PlayParameter,
        player::QuestionPurpose::ResolutionChoice => wire::QuestionPurpose::ResolutionChoice,
        player::QuestionPurpose::RuleChoice => wire::QuestionPurpose::RuleChoice,
        player::QuestionPurpose::QuickWindow => wire::QuestionPurpose::QuickWindow,
        player::QuestionPurpose::MainAction => wire::QuestionPurpose::MainAction,
    };
    // The board owns stage facts; this check only admits supported purpose/stage pairs.
    let stage = observation
        .processing
        .first()
        .ok_or(ProjectionError::Unavailable)?
        .stage;
    match (purpose, stage) {
        (player::QuestionPurpose::QuickWindow, ProcessingStage::Quick)
        | (player::QuestionPurpose::RuleChoice, ProcessingStage::PendingOrder) => {
            Ok((purpose, wire_purpose, stage))
        }
        _ => Err(ProjectionError::Unavailable),
    }
}

pub(super) fn project(
    domain: &DecisionShape,
    context: Context<'_>,
    catalog: &Catalog,
    references: &BattleReferences,
) -> Result<(wire::QuestionUse, wire::QuestionShape, QuestionBinding)> {
    let (usage, shape, binding) = if let DecisionShape::Quick { window, options } = domain {
        let (shape, binding) = quick::project(*window, options, context, catalog, references)?;
        (wire::QuestionUse::Quick, shape, Binding::Quick(binding))
    } else if let DecisionShape::Pending(choices) = domain {
        let (shape, binding) = pending::project(choices, context)?;
        (
            wire::QuestionUse::PendingOrder,
            shape,
            Binding::Pending(binding),
        )
    } else {
        return Err(ProjectionError::Unavailable);
    };
    Ok((usage, shape, QuestionBinding::Flow(binding)))
}

impl Binding {
    pub(super) fn answer(
        &self,
        token: QuestionToken,
        answer: &Answer,
    ) -> result::Result<player::Command, AnswerError> {
        let decoded = match self {
            Self::Quick(binding) => binding.answer(answer)?,
            Self::Pending(binding) => binding.answer(answer)?,
        };
        Ok(player::Command::Answer {
            token,
            answer: decoded,
        })
    }
}
