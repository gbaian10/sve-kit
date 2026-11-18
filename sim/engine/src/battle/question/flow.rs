//! Rule-owned decisions are separate from effect answer domains.
//! F02A owns this family; F02B/F02D keep effect shapes in Decision.

use super::{AnswerError, Binding as QuestionBinding, Result, decision::Context};
use crate::battle::{BattleReferences, ProjectionError};
use crate::catalog::Catalog;
use crate::game::player::{self, DecisionShape, QuestionToken};
use core::result;
use sve_battle_contract::{
    Answer,
    observation::{ProcessingContext, ProcessingStage},
    question as wire,
};

mod pending;
mod quick;
mod trigger_quota;

#[derive(Clone)]
pub(super) enum Binding {
    Quick(quick::Binding),
    Pending(pending::Binding),
    TriggerQuota(trigger_quota::Binding),
}

pub(super) fn context(
    purpose: player::QuestionPurpose,
    processing: &ProcessingContext,
) -> Result<(
    player::QuestionPurpose,
    wire::QuestionPurpose,
    ProcessingStage,
)> {
    let wire_purpose = self::purpose(purpose);
    // The board owns the current producer's stage, including suspended parent frames.
    let stage = processing.stage.ok_or(ProjectionError::Unavailable)?;
    match (purpose, stage) {
        (player::QuestionPurpose::QuickWindow, ProcessingStage::Quick)
        | (
            player::QuestionPurpose::RuleChoice,
            ProcessingStage::PendingOrder | ProcessingStage::TriggerAdmission,
        ) => Ok((purpose, wire_purpose, stage)),
        _ => Err(ProjectionError::Unavailable),
    }
}

pub(super) const fn purpose(purpose: player::QuestionPurpose) -> wire::QuestionPurpose {
    match purpose {
        player::QuestionPurpose::PlayParameter => wire::QuestionPurpose::PlayParameter,
        player::QuestionPurpose::ResolutionChoice => wire::QuestionPurpose::ResolutionChoice,
        player::QuestionPurpose::RuleChoice => wire::QuestionPurpose::RuleChoice,
        player::QuestionPurpose::QuickWindow => wire::QuestionPurpose::QuickWindow,
        player::QuestionPurpose::MainAction => wire::QuestionPurpose::MainAction,
    }
}

pub(super) fn project(
    domain: &DecisionShape,
    context: Context<'_>,
    stage: ProcessingStage,
    catalog: &Catalog,
    references: &BattleReferences,
) -> Result<(wire::QuestionUse, wire::QuestionShape, QuestionBinding)> {
    let (usage, shape, binding) =
        if let (DecisionShape::Quick { window, options }, ProcessingStage::Quick) = (domain, stage)
        {
            let (shape, binding) = quick::project(*window, options, context, catalog, references)?;
            (wire::QuestionUse::Quick, shape, Binding::Quick(binding))
        } else if let (DecisionShape::Pending(choices), ProcessingStage::PendingOrder) =
            (domain, stage)
        {
            let (shape, binding) = pending::project(choices, context)?;
            (
                wire::QuestionUse::PendingOrder,
                shape,
                Binding::Pending(binding),
            )
        } else if let (DecisionShape::TriggerQuota(domain), ProcessingStage::TriggerAdmission) =
            (domain, stage)
        {
            let (shape, binding) = trigger_quota::project(domain, context)?;
            (
                wire::QuestionUse::TriggerQuota,
                shape,
                Binding::TriggerQuota(binding),
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
            Self::TriggerQuota(binding) => binding.answer(answer)?,
        };
        Ok(player::Command::Answer {
            token,
            answer: decoded,
        })
    }
}
