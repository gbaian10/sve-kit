//! Player submissions reuse authority validation and its original rollback boundaries.

use super::{
    Command, Input, Question, QuestionPurpose, QuestionShape, QuestionToken, Submission,
    SubmissionKind, Viewer, Waiting,
};
use crate::game::{Game, internal};
use crate::ir::PlayerId;
use crate::{EngineFailure, Result, invalid};

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

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The player boundary shares the existing game authority."
)]
impl Game {
    /// Hand a trusted authority fixture or action to the player submission pipeline.
    /// This mutating entry is unnecessary between player answers; input itself is read-only.
    pub fn advance_player_input(&mut self) {
        self.enable_player_events();
        if self.player_fault.is_some() {
            return;
        }
        let before = self.clone();
        let published = self.published_knowledge();
        if let Err(error) = self
            .advance_player_rules()
            .and_then(|()| self.validate_player_decision_projection())
        {
            self.player_fault = Some(match self.restore_player_state(before, &published) {
                Ok(()) => error,
                Err(restore) => restore,
            });
        }
        self.publish_player_events();
    }

    /// Inspect already prepared input without changing tokens, randomness or events.
    #[must_use]
    pub fn input(&self, viewer: Viewer) -> Input {
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
            if actor != viewer {
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
            Question {
                token: QuestionToken {
                    epoch: self.state.ir_epoch,
                    serial: self.player_epoch,
                    action: false,
                },
                actor,
                purpose,
                shape: QuestionShape::Decision(shape),
            }
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
                    if actor != viewer {
                        return Input::Waiting(Waiting { actor, dice: None });
                    }
                    let token = QuestionToken {
                        epoch: self.state.ir_epoch,
                        serial: self.player_epoch,
                        action: true,
                    };
                    let Ok(actions) = self.player_main_actions(actor, token) else {
                        return Input::EngineFault;
                    };
                    Question {
                        token,
                        actor,
                        purpose,
                        shape: QuestionShape::ActionWindow(actions),
                    }
                }
            }
        };
        if question.actor == viewer {
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
        let kind = match command {
            Command::Evolution { token, answer } => {
                self.submit_player_evolution(viewer, token, answer)
            }
            Command::Opening { token, answer } => self.submit_player_opening(viewer, token, answer),
            Command::MainAction {
                token,
                intent,
                answer,
            } => self.submit_player_main_action(viewer, token, &intent, answer),
            Command::Closing { token, answer } => self.submit_player_closing(viewer, token, answer),
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
            Command::Answer { token, answer } => self.submit_player_answer(viewer, token, answer),
        };
        if kind != SubmissionKind::Rejected {
            self.publish_player_events();
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
        // An origin rollback can restore an earlier question; its public token must stay dead.
        self.player_epoch = next;
        self.advance_player_input();
        if self.player_fault.is_some() {
            SubmissionKind::EngineFault
        } else {
            kind
        }
    }

    fn advance_player_rules(&mut self) -> Result<()> {
        if !self.player_authority_ready() {
            return Err(invalid(
                "prototype continuation cannot enter the typed player interface",
            ));
        }
        if self.state.game["ended"] == true || self.player_decision().is_some() {
            return Ok(());
        }
        if let Some(actor) = self.evolution_actor() {
            self.player_evolution_question(actor)?
                .ok_or_else(|| invalid("missing evolution question"))?;
            return Ok(());
        }
        if self.state.flow["kind"] == "pregame" {
            self.player_opening_question()?;
            return Ok(());
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
        if self.state.game["ended"] != true
            && self.player_decision().is_none()
            && self.player_action_purpose().is_some()
        {
            let actor = viewer(self.active()).ok_or_else(|| invalid("invalid main actor"))?;
            self.player_main_actions(
                actor,
                QuestionToken {
                    epoch: self.state.ir_epoch,
                    serial: self.player_epoch,
                    action: true,
                },
            )?;
        }
        if self.state.game["ended"] != true
            && self.player_decision().is_none()
            && self.player_action_purpose().is_none()
            && self.player_closing_question()?.is_none()
        {
            return Err(EngineFailure::Unsupported(
                "typed action window for this procedure".into(),
            ));
        }
        Ok(())
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
