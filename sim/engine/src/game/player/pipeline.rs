//! Player submissions reuse authority validation and its original rollback boundaries.

use super::{
    Command, DecisionShape, Input, Question, QuestionPurpose, QuestionShape, QuestionToken,
    Submission, SubmissionKind, Viewer, Waiting,
};
use crate::game::{Game, internal};
use crate::ir::PlayerId;
use crate::{EngineFailure, Result, invalid};
use core::mem;

pub(super) fn viewer(seat: &str) -> Option<Viewer> {
    match seat {
        "P1" => Some(Viewer::P1),
        "P2" => Some(Viewer::P2),
        _ => None,
    }
}

const fn actor(seat: PlayerId) -> Option<Viewer> {
    match seat.seat {
        1 => Some(Viewer::P1),
        2 => Some(Viewer::P2),
        _ => None,
    }
}

impl Game {
    /// Hand a trusted authority fixture or action to the player submission pipeline.
    /// This mutating entry is unnecessary between player answers; input itself is read-only.
    pub fn advance_player_input(&mut self) {
        self.advance_player_input_cached(false);
    }

    pub(super) fn advance_player_input_cached(&mut self, cache: bool) {
        self.enable_player_events();
        if self.player_fault.is_some() {
            return;
        }
        let before = self.clone();
        let published = self.published_knowledge();
        match self
            .advance_player_rules()
            .and_then(|question| self.validate_player_question(question))
        {
            Ok(Some(question)) if cache && self.player_authority_ready() => {
                self.player_inputs = super::InputCache::prepared(question);
            }
            Ok(_) => {}
            Err(error) => {
                self.player_fault = Some(match self.restore_player_state(before, &published) {
                    Ok(()) => error,
                    Err(restore) => restore,
                });
            }
        }
        self.publish_player_events();
    }

    /// Inspect already prepared input without changing tokens, randomness or events.
    #[must_use]
    pub fn input(&self, viewer: Viewer) -> Input {
        self.player_inputs.read(self, viewer)
    }

    pub(super) fn compute_player_input(&self, viewer: Option<Viewer>) -> Input {
        if self.player_fault.is_some() || !self.player_authority_ready() {
            return Input::EngineFault;
        }
        if self.state.game["ended"] == true {
            return match self.state.game["winner"].as_str() {
                Some(seat) => {
                    self::viewer(seat).map_or(Input::EngineFault, |winner| Input::Finished {
                        winner: Some(winner),
                    })
                }
                None if self.state.game["winner"].is_null() => Input::Finished { winner: None },
                None => Input::EngineFault,
            };
        }
        match self.player_evolution_question(viewer) {
            Ok(Some(input)) => return input,
            Err(_) => return Input::EngineFault,
            Ok(None) => {}
        }
        let question = if self.state.flow["kind"] == "pregame" {
            let Ok(question) = self.player_opening_question() else {
                return Input::EngineFault;
            };
            question
        } else if let Some((question, purpose)) = self.player_decision() {
            let Some(actor) = actor(question.actor) else {
                return Input::EngineFault;
            };
            if viewer.is_some_and(|viewer| actor != viewer) {
                let dice = if let internal::InputDomain::DiceReroll { value, remaining } =
                    question.domain
                {
                    Some((value, remaining))
                } else {
                    None
                };
                return Input::Waiting(Waiting { actor, dice });
            }
            let Ok(shape) = self.player_decision_shape() else {
                return Input::EngineFault;
            };
            self.decision_question(actor, purpose, shape)
        } else {
            if self.typed.is_some() || self.internal_card_stage().is_some() {
                return Input::EngineFault;
            }
            match self.player_closing_question() {
                Ok(Some(question)) => question,
                Err(_) => return Input::EngineFault,
                Ok(None) => {
                    let Some(actor) = self::viewer(self.active()) else {
                        return Input::EngineFault;
                    };
                    let Some(purpose) = self.player_action_purpose() else {
                        return Input::EngineFault;
                    };
                    if viewer.is_some_and(|viewer| actor != viewer) {
                        return Input::Waiting(Waiting { actor, dice: None });
                    }
                    let Ok(question) = self.main_question(actor, purpose) else {
                        return Input::EngineFault;
                    };
                    question
                }
            }
        };
        if viewer.is_none_or(|viewer| question.actor == viewer) {
            return Input::Question(Box::new(question));
        }
        Input::Waiting(Waiting {
            actor: question.actor,
            dice: None,
        })
    }

    /// Submit an authenticated answer or surrender, then prepare the next input point.
    #[must_use]
    pub fn submit_player(&mut self, viewer: Viewer, command: Command) -> Submission {
        let inputs = mem::take(&mut self.player_inputs);
        let kind = match command {
            Command::Evolution { token, answer } => {
                self.submit_player_evolution(viewer, token, answer, &inputs)
            }
            Command::Opening { token, answer } => {
                self.submit_player_opening(viewer, token, answer, &inputs)
            }
            Command::MainAction {
                token,
                intent,
                answer,
            } => self.submit_player_main_action(viewer, token, &intent, answer, &inputs),
            Command::Closing { token, answer } => {
                self.submit_player_closing(viewer, token, answer, &inputs)
            }
            Command::Concede => {
                if self.state.game["ended"] == true {
                    SubmissionKind::Rejected
                } else {
                    match self.concede_internal(viewer.player()) {
                        Ok(_terminal) => {
                            self.player_evolution = None;
                            self.player_fault = None;
                            SubmissionKind::Applied
                        }
                        Err(error) => {
                            self.player_fault = Some(error);
                            SubmissionKind::EngineFault
                        }
                    }
                }
            }
            Command::Answer { token, answer } => {
                self.submit_player_answer(viewer, token, answer, inputs.enabled())
            }
            Command::Withdraw { token } => self.submit_player_withdraw(viewer, token, &inputs),
        };
        if kind != SubmissionKind::Rejected {
            self.publish_player_events();
        }
        if kind == SubmissionKind::Rejected || !self.player_inputs.enabled() {
            self.player_inputs = inputs.after_submission(kind);
        }
        Submission {
            kind,
            input: self.input(viewer),
        }
    }

    fn submit_player_answer(
        &mut self,
        viewer: Viewer,
        token: QuestionToken,
        answer: super::Answer,
        cache: bool,
    ) -> SubmissionKind {
        if self.player_fault.is_some() {
            return SubmissionKind::EngineFault;
        }
        let Some((question, _)) = self.player_decision() else {
            return SubmissionKind::Rejected;
        };
        if token.action
            || token.epoch != self.state.ir_epoch
            || token.serial != self.player_epoch
            || viewer.player() != question.actor
        {
            return SubmissionKind::Rejected;
        }
        let Some(next) = self.player_epoch.checked_add(1) else {
            self.player_fault = Some(invalid("player input epoch exhausted"));
            return SubmissionKind::EngineFault;
        };
        let mut validated = false;
        let result =
            self.answer_player_decision(question.actor, question.token, answer, &mut validated);
        let kind = match result {
            Ok(kind) => kind,
            Err(EngineFailure::IllegalPlay(_)) if validated => SubmissionKind::Reverted,
            Err(EngineFailure::IllegalPlay(_) | EngineFailure::Invalid(_)) if !validated => {
                return SubmissionKind::Rejected;
            }
            Err(error) => {
                self.player_epoch = next;
                self.player_fault = Some(error);
                return SubmissionKind::EngineFault;
            }
        };
        // This answer did not fault, so a restoration inside it was a rules rollback.
        self.player_events.settle_presentation(false);
        // An origin rollback can restore an earlier question; its public token must stay dead.
        self.player_epoch = next;
        self.advance_player_input_cached(cache);
        if self.player_fault.is_some() {
            SubmissionKind::EngineFault
        } else {
            kind
        }
    }

    fn submit_player_withdraw(
        &mut self,
        viewer: Viewer,
        token: QuestionToken,
        inputs: &super::InputCache,
    ) -> SubmissionKind {
        if self.player_fault.is_some() {
            return SubmissionKind::EngineFault;
        }
        let Input::Question(question) = inputs.read(self, viewer) else {
            return SubmissionKind::Rejected;
        };
        if question.token != token || !question.withdrawable {
            return SubmissionKind::Rejected;
        }
        let Some(next) = self.player_epoch.checked_add(1) else {
            self.player_fault = Some(invalid("player input epoch exhausted"));
            return SubmissionKind::EngineFault;
        };
        // Evolution stages only record choices, so dropping them is its whole origin.
        if self.player_evolution.take().is_none()
            && let Err(error) = self.withdraw_declaration()
        {
            self.player_epoch = next;
            self.player_fault = Some(error);
            return SubmissionKind::EngineFault;
        }
        self.player_events.settle_presentation(false);
        // The restored action window is a new occurrence; the withdrawn token stays dead.
        self.player_epoch = next;
        self.advance_player_input_cached(inputs.enabled());
        if self.player_fault.is_some() {
            SubmissionKind::EngineFault
        } else {
            SubmissionKind::Reverted
        }
    }

    fn advance_player_rules(&mut self) -> Result<Option<Question>> {
        if !self.player_authority_ready() {
            return Err(invalid(
                "prototype continuation cannot enter the typed player interface",
            ));
        }
        if self.state.game["ended"] == true || self.player_decision().is_some() {
            return Ok(None);
        }
        if let Some(actor) = self.evolution_actor() {
            let Some(Input::Question(question)) = self.player_evolution_question(Some(actor))?
            else {
                return Err(invalid("missing evolution question"));
            };
            return Ok(Some(*question));
        }
        if self.state.flow["kind"] == "pregame" {
            return self.player_opening_question().map(Some);
        }
        if self.typed.is_some() || !self.prepared.is_empty() {
            return Err(invalid("unfinished authority execution at player handoff"));
        }
        if self.internal_card_stage() == Some(internal::ResumeStage::CheckTiming) {
            self.resume_internal_card()?;
        } else {
            self.checks()?;
        }
        if self.state.game["ended"] != true && self.internal_pending_choice()?.is_none() {
            self.internal_quick_choice()?;
        }
        if self.state.game["ended"] == true || self.player_decision().is_some() {
            return Ok(None);
        }
        let question = if let Some(purpose) = self.player_action_purpose() {
            let actor = viewer(self.active()).ok_or_else(|| invalid("invalid main actor"))?;
            self.main_question(actor, purpose)?
        } else {
            self.player_closing_question()?.ok_or_else(|| {
                EngineFailure::Unsupported("typed action window for this procedure".into())
            })?
        };
        // A validated action window can only be published once the continuation is idle.
        if self.typed.is_some() || self.internal_card_stage().is_some() {
            return Ok(None);
        }
        Ok(Some(question))
    }

    fn validate_player_question(&self, question: Option<Question>) -> Result<Option<Question>> {
        if self.state.game["ended"] != true
            && let Some((decision, purpose)) = self.player_decision()
        {
            let shape = self.player_decision_shape()?;
            // The native input still gives evolution and opening priority over a decision.
            if self.evolution_actor().is_none() && self.state.flow["kind"] != "pregame" {
                return Ok(actor(decision.actor)
                    .map(|actor| self.decision_question(actor, purpose, shape)));
            }
        }
        Ok(question)
    }

    fn main_question(&self, actor: Viewer, purpose: QuestionPurpose) -> Result<Question> {
        let token = QuestionToken {
            epoch: self.state.ir_epoch,
            serial: self.player_epoch,
            action: true,
        };
        Ok(Question {
            token,
            actor,
            purpose,
            shape: QuestionShape::ActionWindow(self.player_main_actions(actor, token)?),
            withdrawable: false,
        })
    }

    fn decision_question(
        &self,
        actor: Viewer,
        purpose: QuestionPurpose,
        shape: DecisionShape,
    ) -> Question {
        Question {
            token: QuestionToken {
                epoch: self.state.ir_epoch,
                serial: self.player_epoch,
                action: false,
            },
            actor,
            purpose,
            shape: QuestionShape::Decision(shape),
            withdrawable: self.withdrawable_declaration(),
        }
    }

    const fn player_authority_ready(&self) -> bool {
        self.state.frame.is_none() && self.state.prompt.is_none() && self.state.pending.is_empty()
    }

    fn player_action_purpose(&self) -> Option<QuestionPurpose> {
        match (
            self.state.flow["kind"].as_str(),
            self.state.flow["stage"].as_str(),
        ) {
            (Some("main"), _) if self.state.turn["phase"] == "main" => {
                Some(QuestionPurpose::MainAction)
            }
            _ => None,
        }
    }
}
