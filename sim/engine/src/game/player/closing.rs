//! Turn-closing choices reuse the rule primitives after capability and cardinality checks.

use super::observation::{CardObservation, seat, viewer};
use super::{
    Input, ObjectHandle, Question, QuestionPurpose, QuestionShape, QuestionToken, SubmissionKind,
    Viewer,
};
use crate::game::Game;
use crate::{EngineFailure, Result, invalid};
use alloc::collections::BTreeSet;

/// A turn-closing procedure; entry Ward remains a separate typed decision domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosingQuestion {
    /// Act any subset of these standing Guard followers, including none.
    GuardAct(Vec<CardObservation>),
    /// Discard exactly the excess over the hand limit, before pending abilities resolve.
    Discard {
        /// Current own hand, addressed by this viewer's object capabilities.
        cards: Vec<CardObservation>,
        /// Exact number of cards required by the existing hand-limit rule.
        count: u64,
    },
}

/// A capability-based answer to one specific closing procedure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosingAnswer {
    /// Act the selected standing Guard followers; not an entry Ward decision.
    GuardAct(Vec<ObjectHandle>),
    /// Discard the selected excess cards from the active player's hand.
    Discard(Vec<ObjectHandle>),
}

enum Action {
    EndPhase,
    GuardAct(Vec<String>),
    Discard(Vec<String>),
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Closing adapters share the existing rule authority."
)]
impl Game {
    pub(super) fn player_closing_question(&self) -> Result<Option<Question>> {
        if self.state.flow["kind"] != "end" {
            return Ok(None);
        }
        let actor = viewer(self.active())?;
        let shape = match self.state.flow["stage"].as_str() {
            Some("guard") => {
                ClosingQuestion::GuardAct(self.closing_cards(actor, &self.player_end_guards()?)?)
            }
            Some("discard") => ClosingQuestion::Discard {
                cards: self.closing_cards(actor, &self.zone_ids(self.active(), "hand"))?,
                count: u64::try_from(
                    self.zone_count(self.active(), "hand")
                        .saturating_sub(7)
                        .max(0),
                )
                .map_err(invalid)?,
            },
            _ => return Ok(None),
        };
        Ok(Some(Question {
            token: QuestionToken {
                epoch: self.state.ir_epoch,
                serial: self.player_epoch,
                action: true,
            },
            actor,
            purpose: QuestionPurpose::RuleChoice,
            shape: QuestionShape::Closing(shape),
        }))
    }

    fn player_end_guards(&self) -> Result<Vec<String>> {
        self.zone_ids(self.active(), "field")
            .into_iter()
            .filter_map(|id| {
                match self.object(&id).and_then(|object| {
                    if object.state["acted"] == true {
                        Ok(false)
                    } else {
                        self.keywords(&id)
                            .map(|keywords| keywords.contains("guard"))
                    }
                }) {
                    Ok(true) => Some(Ok(id)),
                    Ok(false) => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .collect()
    }

    fn closing_cards(&self, actor: Viewer, ids: &[String]) -> Result<Vec<CardObservation>> {
        ids.iter()
            .map(|id| {
                let handle = self
                    .state
                    .knowledge
                    .get(seat(actor))
                    .and_then(|known| known.handle(id))
                    .ok_or_else(|| invalid("closing candidate is not identifiable"))?;
                self.resolve_player_object(actor, &handle)?;
                Ok(CardObservation {
                    handle,
                    properties: self.observed_card_properties(actor, id)?,
                })
            })
            .collect()
    }

    pub(super) fn submit_player_end_phase(
        &mut self,
        actor: Viewer,
        token: QuestionToken,
    ) -> SubmissionKind {
        self.submit_player_closing_action(actor, token, None)
    }

    pub(super) fn submit_player_closing(
        &mut self,
        actor: Viewer,
        token: QuestionToken,
        answer: ClosingAnswer,
    ) -> SubmissionKind {
        self.submit_player_closing_action(actor, token, Some(answer))
    }

    fn closing_action(
        &self,
        actor: Viewer,
        shape: &QuestionShape,
        answer: Option<ClosingAnswer>,
    ) -> Result<Option<Action>> {
        match (shape, answer) {
            (QuestionShape::ActionWindow, None)
                if self.state.flow["kind"] == "main" && self.state.turn["phase"] == "main" =>
            {
                Ok((!self.attack_required()?).then_some(Action::EndPhase))
            }
            (
                QuestionShape::Closing(ClosingQuestion::GuardAct(_)),
                Some(ClosingAnswer::GuardAct(handles)),
            ) => {
                let ids = self.closing_selection(actor, &handles, &self.player_end_guards()?)?;
                Ok(Some(Action::GuardAct(ids)))
            }
            (
                QuestionShape::Closing(ClosingQuestion::Discard { count, .. }),
                Some(ClosingAnswer::Discard(handles)),
            ) => {
                if u64::try_from(handles.len()).map_err(invalid)? != *count {
                    return Ok(None);
                }
                let ids =
                    self.closing_selection(actor, &handles, &self.zone_ids(self.active(), "hand"))?;
                Ok(Some(Action::Discard(ids)))
            }
            _ => Ok(None),
        }
    }

    fn closing_selection(
        &self,
        actor: Viewer,
        handles: &[ObjectHandle],
        candidates: &[String],
    ) -> Result<Vec<String>> {
        let ids = handles
            .iter()
            .map(|handle| self.resolve_player_object(actor, handle).map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        let unique = ids.iter().collect::<BTreeSet<_>>();
        if unique.len() != ids.len() || ids.iter().any(|id| !candidates.contains(id)) {
            return Err(invalid("invalid closing selection"));
        }
        Ok(ids)
    }

    fn submit_player_closing_action(
        &mut self,
        actor: Viewer,
        token: QuestionToken,
        answer: Option<ClosingAnswer>,
    ) -> SubmissionKind {
        if self.player_fault.is_some() {
            return SubmissionKind::EngineFault;
        }
        let input = self.input(actor);
        let Input::Question(question) = input else {
            return if matches!(input, Input::EngineFault) {
                SubmissionKind::EngineFault
            } else {
                SubmissionKind::Rejected
            };
        };
        if question.actor != actor || question.token != token {
            return SubmissionKind::Rejected;
        }
        let action = match self.closing_action(actor, &question.shape, answer) {
            Ok(Some(action)) => action,
            Ok(None) | Err(EngineFailure::Invalid(_)) => return SubmissionKind::Rejected,
            Err(error) => {
                self.player_fault = Some(error);
                return SubmissionKind::EngineFault;
            }
        };
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
        self.node = format!("player-rule-{}", self.state.next_decision);
        self.state.next_decision = self.state.next_decision.saturating_add(1);
        let result = match action {
            Action::EndPhase => self.start_end(),
            Action::GuardAct(ids) => self.apply_end_guard(&ids),
            Action::Discard(ids) => self.apply_end_discard(&ids),
        };
        if let Err(error) = result {
            self.player_fault = Some(match self.restore_player_state(before, &published) {
                Ok(()) => error,
                Err(restore) => restore,
            });
            self.player_epoch = next;
            return SubmissionKind::EngineFault;
        }
        self.player_epoch = next;
        self.advance_player_input();
        if self.player_fault.is_some() {
            SubmissionKind::EngineFault
        } else {
            SubmissionKind::Applied
        }
    }
}
