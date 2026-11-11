//! Normal evolution keeps its rule payment separate from effect evolution.

use super::observation::CardObservation;
use super::{
    Input, Question, QuestionPurpose, QuestionShape, QuestionToken, SubmissionKind, Viewer,
};
use crate::game::Game;
use crate::game::internal::TargetDomain;
use crate::{EngineFailure, Result, invalid};
use core::fmt;

/// An opaque face choice bound to one evolution question.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EvolutionFormId {
    token: QuestionToken,
    ordinal: usize,
}

impl fmt::Debug for EvolutionFormId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvolutionFormId").finish_non_exhaustive()
    }
}

impl EvolutionFormId {
    pub(in crate::game) const fn new(token: QuestionToken, ordinal: usize) -> Self {
        Self { token, ordinal }
    }

    pub(in crate::game) fn ordinal(&self, token: QuestionToken) -> Result<usize> {
        if self.token != token {
            return Err(invalid("retired evolution form choice"));
        }
        Ok(self.ordinal)
    }
}

/// One authorized physical evolution card and destination face, without payment combinations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvolutionForm {
    /// Return with the exact current question token.
    pub id: EvolutionFormId,
    /// Card currently identifiable in the actor's evolution deck.
    pub card: CardObservation,
    /// Destination face on that card; may differ from its current face.
    pub face: u64,
    /// Rules name of that destination face.
    pub name: String,
}

/// Resources of normal evolution; other printed costs remain native payment instructions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvolutionPayment {
    /// Adjusted total: the answer's PP plus EP must equal this amount.
    pub total_pp: i64,
    /// Available PP; PP in the answer must be nonnegative and no larger.
    pub available_pp: i64,
    /// At most one EP can replace one PP, and only when the total is positive.
    pub ep_allowed: bool,
    /// One SEP may be paid for super evolution at this rules timing.
    pub super_allowed: bool,
}

/// One stage of a normal evolution declaration, always a play parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvolutionQuestion {
    /// Choose exactly one destination, in authorized card/face/name/handle order.
    Form(Vec<EvolutionForm>),
    /// Select the printed material count using the existing material domain.
    Materials(TargetDomain),
    /// Select the printed own-hand discard count using the existing discard domain.
    Discard(TargetDomain),
    /// Select one PP/EP payment and whether to use SEP; fixed printed costs are automatic.
    Payment(EvolutionPayment),
}

/// A typed answer to one normal evolution stage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvolutionAnswer {
    /// Exactly one face capability from the current question.
    Form(EvolutionFormId),
    /// Question-local material handles, with the displayed cardinality.
    Materials(Vec<u64>),
    /// Question-local own-hand handles, with the displayed cardinality.
    Discard(Vec<u64>),
    /// PP plus zero or one EP must equal the quoted total; SEP chooses super evolution.
    Payment {
        /// Nonnegative play points to pay.
        pp: i64,
        /// Substitute one EP for one PP.
        ep: bool,
        /// Pay one SEP for super evolution.
        super_evolution: bool,
    },
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Evolution answers share player rollback and publication."
)]
impl Game {
    pub(super) fn player_evolution_question(&self, viewer: Viewer) -> Result<Option<Input>> {
        let Some(actor) = self.evolution_actor() else {
            return Ok(None);
        };
        if actor != viewer {
            return Ok(Some(Input::Waiting(super::Waiting { actor, dice: None })));
        }
        let token = QuestionToken {
            epoch: self.state.ir_epoch,
            serial: self.player_epoch,
            action: true,
        };
        Ok(Some(Input::Question(Box::new(Question {
            actor,
            token,
            purpose: QuestionPurpose::PlayParameter,
            shape: QuestionShape::Evolution(self.evolution_view(token)?),
        }))))
    }

    pub(super) fn submit_player_evolution(
        &mut self,
        actor: Viewer,
        token: QuestionToken,
        answer: EvolutionAnswer,
    ) -> SubmissionKind {
        if self.player_fault.is_some() {
            return SubmissionKind::EngineFault;
        }
        let Input::Question(question) = self.input(actor) else {
            return SubmissionKind::Rejected;
        };
        if question.actor != actor
            || question.token != token
            || !matches!(question.shape, QuestionShape::Evolution(_))
        {
            return SubmissionKind::Rejected;
        }
        if let Err(error) = self.check_internal_rule_input() {
            self.player_fault = Some(error);
            return SubmissionKind::EngineFault;
        }
        let Some(next) = self.player_epoch.checked_add(1) else {
            self.player_fault = Some(invalid("player input epoch exhausted"));
            return SubmissionKind::EngineFault;
        };
        let before = self.clone();
        let published = self.published_knowledge();
        let mut validated = false;
        let result = self.answer_main_evolution(token, answer, &mut validated);
        let kind = match result {
            Ok(true) => SubmissionKind::Applied,
            Err(EngineFailure::Invalid(_)) if !validated => {
                *self = before;
                return SubmissionKind::Rejected;
            }
            Ok(false) | Err(EngineFailure::IllegalPlay(_)) if validated => {
                if let Err(error) = self.restore_player_state(before, &published) {
                    self.player_fault = Some(error);
                    return SubmissionKind::EngineFault;
                }
                self.player_evolution = None;
                SubmissionKind::Reverted
            }
            Ok(false) => {
                self.player_fault = Some(invalid("evolution answer classification"));
                return SubmissionKind::EngineFault;
            }
            Err(error) => {
                self.player_fault = Some(match self.restore_player_state(before, &published) {
                    Ok(()) => error,
                    Err(restore) => restore,
                });
                self.player_epoch = next;
                return SubmissionKind::EngineFault;
            }
        };
        self.player_epoch = next;
        self.advance_player_input();
        if self.player_fault.is_some() {
            SubmissionKind::EngineFault
        } else {
            kind
        }
    }
}
