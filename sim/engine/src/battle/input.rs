//! One stop binds the complete board, visible input and private answer capability.

use super::{AnswerError, QuestionProjection, question::BoundAnswer};
use crate::game::player::{self, Viewer};
use core::fmt;
use sve_battle_contract::{
    Answer, BattleInput, Command, QuestionId, observation::BattleObservation,
};

/// One authenticated seat's immutable input and board, before any submission.
/// Native tokens and intent bindings are never serialized or returned separately.
#[derive(Clone)]
pub struct InputProjection {
    observation: BattleObservation,
    input: BattleInput,
    answer: Option<BoundAnswer>,
    viewer: Viewer,
    can_concede: bool,
}

impl fmt::Debug for InputProjection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputProjection").finish_non_exhaustive()
    }
}

impl InputProjection {
    pub(crate) fn question(
        question: QuestionProjection,
        viewer: Viewer,
        can_concede: bool,
    ) -> Self {
        let (observation, question, answer) = question.into_parts();
        Self {
            observation,
            input: BattleInput::Question {
                question: Box::new(question),
            },
            answer: Some(answer),
            viewer,
            can_concede,
        }
    }

    pub(crate) const fn passive(
        observation: BattleObservation,
        input: BattleInput,
        viewer: Viewer,
        can_concede: bool,
    ) -> Self {
        Self {
            observation,
            input,
            answer: None,
            viewer,
            can_concede,
        }
    }

    /// Complete board from the same stable authority stop as the input.
    #[must_use]
    pub const fn observation(&self) -> &BattleObservation {
        &self.observation
    }

    /// Wire input, with no native continuation or question capability.
    #[must_use]
    pub const fn input(&self) -> &BattleInput {
        &self.input
    }

    /// Decode this question only; waiting, finished and faulted inputs accept no answer.
    ///
    /// # Errors
    /// Rejects a different seat, question, shape or candidate before native submission.
    pub fn answer(
        &self,
        viewer: Viewer,
        id: &QuestionId,
        answer: &Answer,
    ) -> Result<player::Command, AnswerError> {
        self.answer
            .as_ref()
            .ok_or(AnswerError)?
            .answer(viewer, id, answer)
    }

    /// Decode an authenticated wire command without changing the authority.
    ///
    /// # Errors
    /// Rejects invalid answers, a different viewer, a withdrawal from a question that is not
    /// withdrawable, and concession after termination or when the native authority cannot
    /// accept it.
    pub fn command(
        &self,
        viewer: Viewer,
        command: &Command,
    ) -> Result<player::Command, AnswerError> {
        if viewer != self.viewer {
            return Err(AnswerError);
        }
        match command {
            Command::Answer {
                question_id,
                answer,
            } => self.answer(viewer, question_id, answer),
            Command::Withdraw { question_id } => self
                .answer
                .as_ref()
                .ok_or(AnswerError)?
                .withdraw(viewer, question_id),
            Command::Concede if self.can_concede => Ok(player::Command::Concede),
            Command::Concede => Err(AnswerError),
        }
    }
}
