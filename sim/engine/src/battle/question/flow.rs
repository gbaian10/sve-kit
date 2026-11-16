//! Rule-owned decisions are separate from effect answer domains.

use super::{AnswerError, Binding as QuestionBinding, Result, decision::Context};
use crate::battle::{BattleReferences, ProjectionError};
use crate::catalog::Catalog;
use crate::game::player::{self, DecisionShape, QuestionToken};
use core::result;
use sve_battle_contract::{Answer, observation::ProcessingStage, question as wire};

mod quick;

#[derive(Clone)]
pub(super) enum Binding {
    Quick(quick::Binding),
}

pub(super) const fn context(
    purpose: player::QuestionPurpose,
) -> Result<(
    player::QuestionPurpose,
    wire::QuestionPurpose,
    ProcessingStage,
)> {
    match purpose {
        player::QuestionPurpose::QuickWindow => Ok((
            purpose,
            wire::QuestionPurpose::QuickWindow,
            ProcessingStage::Quick,
        )),
        // Other rule stops still lack their complete occurrence context.
        player::QuestionPurpose::PlayParameter
        | player::QuestionPurpose::ResolutionChoice
        | player::QuestionPurpose::RuleChoice
        | player::QuestionPurpose::MainAction => Err(ProjectionError::Unavailable),
    }
}

pub(super) fn project(
    domain: &DecisionShape,
    context: Context<'_>,
    catalog: &Catalog,
    references: &BattleReferences,
) -> Result<(wire::QuestionUse, wire::QuestionShape, QuestionBinding)> {
    let DecisionShape::Quick { window, options } = domain else {
        return Err(ProjectionError::Unavailable);
    };
    let (shape, binding) = quick::project(*window, options, context, catalog, references)?;
    Ok((
        wire::QuestionUse::Quick,
        shape,
        QuestionBinding::Flow(Binding::Quick(binding)),
    ))
}

impl Binding {
    pub(super) fn answer(
        &self,
        token: QuestionToken,
        answer: &Answer,
    ) -> result::Result<player::Command, AnswerError> {
        let decoded = match self {
            Self::Quick(binding) => binding.answer(answer)?,
        };
        Ok(player::Command::Answer {
            token,
            answer: decoded,
        })
    }
}
