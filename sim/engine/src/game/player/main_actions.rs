//! Main intents expose parameter domains while shared primitives retain authority.

use super::observation::{CardObservation, seat};
use super::{Input, ObjectHandle, QuestionShape, QuestionToken, SubmissionKind, Viewer};
use crate::game::{Game, other};
use crate::{EngineFailure, Result, invalid};
use core::fmt;

/// A question-local main-action option, unrelated to authority object identities.
/// This in-process capability has no serialization or wire representation.
#[derive(Clone, PartialEq, Eq)]
pub struct MainIntentId {
    token: QuestionToken,
    source: Option<ObjectHandle>,
    generation: Option<u64>,
}

impl fmt::Debug for MainIntentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MainIntentId").finish_non_exhaustive()
    }
}

/// One authorized attack recipient; card capabilities link to the current observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttackTarget {
    /// An opposing leader.
    Leader(Viewer),
    /// A currently identifiable opposing field card.
    Card(ObjectHandle),
}

/// Parameters for a selected main intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainActionAnswer {
    /// An intent without parameters.
    None,
    /// One recipient from the attack intent's domain.
    Attack(AttackTarget),
}

/// Supported main intents; card play, activation and evolution are not yet listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainIntentKind {
    /// Declare one attack, choosing its target separately.
    Attack {
        /// Authorized attacking card.
        attacker: Box<CardObservation>,
        /// Legal recipients, with the leader first and cards in public field order.
        targets: Vec<AttackTarget>,
    },
    /// End the main phase when there is no outstanding attack obligation.
    EndPhase,
}

/// One main-action option, without an expansion of all complete answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainIntent {
    /// Return this opaque option with the current question token.
    pub id: MainIntentId,
    /// Action and its parameter domain.
    pub kind: MainIntentKind,
}

/// Current attack intents in own field order, followed by an optional `EndPhase`.
/// Card play, activation and evolution are not yet included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainActions {
    /// Options for this question's authenticated actor only.
    pub intents: Vec<MainIntent>,
}

enum Action {
    Attack { attacker: String, target: String },
    EndPhase,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Main intents share authority validation and its rollback boundary."
)]
impl Game {
    pub(super) fn player_main_actions(
        &self,
        actor: Viewer,
        token: QuestionToken,
    ) -> Result<MainActions> {
        let opponent = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let leader = format!("{}.leader", other(seat(actor)));
        let mut intents = Vec::new();
        for id in self.zone_ids(seat(actor), "field") {
            if !self.object_type(&id)?.contains("フォロワー") {
                continue;
            }
            let mut targets = Vec::new();
            if self.can_attack(&id, &leader)? {
                targets.push(AttackTarget::Leader(opponent));
            }
            for target in self.zone_ids(seat(opponent), "field") {
                if self.can_attack(&id, &target)? {
                    targets.push(AttackTarget::Card(self.main_object(actor, &target)?.handle));
                }
            }
            if !targets.is_empty() {
                let attacker = self.main_object(actor, &id)?;
                intents.push(MainIntent {
                    id: MainIntentId {
                        token,
                        source: Some(attacker.handle.clone()),
                        generation: Some(self.object(&id)?.generation),
                    },
                    kind: MainIntentKind::Attack {
                        attacker: Box::new(attacker),
                        targets,
                    },
                });
            }
        }
        if !self.attack_required()? {
            intents.push(MainIntent {
                id: MainIntentId {
                    token,
                    source: None,
                    generation: None,
                },
                kind: MainIntentKind::EndPhase,
            });
        }
        Ok(MainActions { intents })
    }

    fn main_object(&self, actor: Viewer, id: &str) -> Result<CardObservation> {
        let handle = self
            .state
            .knowledge
            .get(seat(actor))
            .and_then(|knowledge| knowledge.handle(id))
            .ok_or_else(|| invalid("main candidate is not identifiable"))?;
        self.resolve_player_object(actor, &handle)?;
        Ok(CardObservation {
            handle,
            properties: self.observed_card_properties(actor, id)?,
        })
    }

    fn main_action(
        &self,
        actor: Viewer,
        intent: &MainIntent,
        answer: MainActionAnswer,
    ) -> Result<Option<Action>> {
        match (&intent.kind, answer) {
            (MainIntentKind::EndPhase, MainActionAnswer::None) => {
                Ok((!self.attack_required()?).then_some(Action::EndPhase))
            }
            (MainIntentKind::Attack { attacker, targets }, MainActionAnswer::Attack(target))
                if targets.contains(&target) =>
            {
                let attacker = self
                    .resolve_player_object(actor, &attacker.handle)?
                    .to_owned();
                let target = match target {
                    AttackTarget::Leader(viewer) => format!("{}.leader", seat(viewer)),
                    AttackTarget::Card(handle) => {
                        self.resolve_player_object(actor, &handle)?.to_owned()
                    }
                };
                Ok(self
                    .can_attack(&attacker, &target)?
                    .then_some(Action::Attack { attacker, target }))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn submit_player_main_action(
        &mut self,
        actor: Viewer,
        token: QuestionToken,
        intent: &MainIntentId,
        answer: MainActionAnswer,
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
        let QuestionShape::ActionWindow(actions) = question.shape else {
            return SubmissionKind::Rejected;
        };
        let Some(selected) = actions
            .intents
            .iter()
            .find(|candidate| &candidate.id == intent)
        else {
            return SubmissionKind::Rejected;
        };
        if self.state.flow["kind"] != "main" || self.state.turn["phase"] != "main" {
            return SubmissionKind::Rejected;
        }
        let action = match self.main_action(actor, selected, answer) {
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
            Action::Attack { attacker, target } => self.apply_attack(&attacker, &target, None),
            Action::EndPhase => self.start_end().map(|()| true),
        };
        match result {
            Ok(true) => {}
            Ok(false) => {
                *self = before;
                return SubmissionKind::Rejected;
            }
            Err(error) => {
                self.player_fault = Some(match self.restore_player_state(before, &published) {
                    Ok(()) => error,
                    Err(restore) => restore,
                });
                self.player_epoch = next;
                return SubmissionKind::EngineFault;
            }
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
