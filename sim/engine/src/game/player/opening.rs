//! Trusted deck input and capability-based answers for the existing opening rules.

use super::observation::{CardObservation, seat};
use super::{
    ObjectHandle, Question, QuestionPurpose, QuestionShape, QuestionToken, StrictStartError,
    SubmissionKind, Viewer,
};
use crate::catalog::SelectedEffects;
use crate::game::Game;
use crate::{EngineFailure, Result, invalid};
use alloc::collections::BTreeSet;
use core::result;
use serde_json::{Value, json};

/// Construction basis already checked by the host against the snapshot's construction index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeckConstruction {
    /// Class construction, with the snapshot's printed class value.
    Class(String),
    /// Title construction, with a registered engine title code.
    Title(String),
}

/// A trusted host's validated deck, containing snapshot printing references, never card facts.
/// Tournament construction, leader eligibility and restrictions belong to the host validator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerDeck {
    /// Validated construction basis.
    pub construction: DeckConstruction,
    /// One snapshot printing reference per physical main-deck card.
    pub main: Vec<String>,
    /// One snapshot printing reference per physical evolve-deck card.
    pub evolve: Vec<String>,
}

/// Only the current seat receives opening candidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpeningQuestion {
    /// Choose one eligible Start Amulet; the other seat cannot see the candidates.
    StartAmulet(Vec<CardObservation>),
    /// The designated chooser selects which seat plays first.
    FirstPlayer,
    /// Keep the hand, or return all of these cards in an explicit bottom order.
    Mulligan(Vec<CardObservation>),
}

/// Answers to the strictly serial opening procedure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpeningAnswer {
    /// Place this eligible Start Amulet face down.
    StartAmulet(ObjectHandle),
    /// Select the first player.
    FirstPlayer(Viewer),
    /// Keep the entire opening hand.
    Keep,
    /// Return the entire hand to the bottom in this order, then draw four cards.
    Mulligan(Vec<ObjectHandle>),
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Opening adapters share the game authority."
)]
impl Game {
    /// Create a typed game from host-validated decks and a source-checked effect selection.
    /// The host keeps the seed private and supplies the already determined first chooser.
    /// No arbitrary position, anonymous cards, physical identities or prototype programs enter.
    ///
    /// # Errors
    /// Returns owner-scoped admission gaps or an authority-only initialization failure.
    /// These startup failures are distinct from a later submission's `EngineFault` result.
    pub fn start_player_game(
        selected: SelectedEffects,
        decks: [PlayerDeck; 2],
        first_chooser: Viewer,
        seed: &str,
    ) -> result::Result<Self, StrictStartError> {
        let (admitted, report) = Self::prepare_strict_player_decks(&selected, &decks)?;
        if !report.is_admitted() {
            return Err(StrictStartError::Admission(report));
        }
        let mut players = serde_json::Map::new();
        for (perspective, deck) in [Viewer::P1, Viewer::P2].into_iter().zip(decks) {
            let cards = |zone: &str, numbers: &[String]| {
                numbers.iter().enumerate().map(|(index, number)| {
                    json!({"id":format!("{}/{zone}/{index}", seat(perspective)),"card":number})
                }).collect::<Vec<_>>()
            };
            let mut player = json!({"leader":{},"deck_list":cards("deck", &deck.main),
                "evolve_deck_list":cards("evolve_deck", &deck.evolve)});
            match deck.construction {
                DeckConstruction::Class(class) => {
                    player["construction"] = json!("class");
                    player["leader"]["class"] = json!(class);
                }
                DeckConstruction::Title(title) => {
                    player["construction"] = json!("title");
                    player["title_code"] = json!(title);
                }
            }
            players.insert(seat(perspective).into(), player);
        }
        let setup = json!({"pregame":true,"players":players});
        let random = json!({"first_chooser":seat(first_chooser)});
        let mut game = Self::unopened(selected.prototypes, &setup, &Value::Null, &random, seed)?;
        game.card_runtime = admitted.card_runtime;
        game.initialize_opening()?;
        // Validate the complete opening hand requirement before returning a live game.
        for player in ["P1", "P2"] {
            let reserved = usize::from(
                !game.start_amulets(player)?.is_empty()
                    && game.state.flow["queue"]
                        .as_array()
                        .is_some_and(|queue| queue.iter().any(|value| value == player)),
            );
            if game.zone_ids(player, "deck").len().saturating_sub(reserved) < 4 {
                return Err(invalid("opening deck has fewer than four drawable cards").into());
            }
        }
        game.player_opening_question()?;
        game.enable_player_events();
        Ok(game)
    }

    pub(super) fn player_opening_question(&self) -> Result<Question> {
        if self.typed.is_some()
            || !self.prepared.is_empty()
            || self.internal_card_stage().is_some()
            || self.player_decision().is_some()
        {
            return Err(invalid("opening overlaps an authority continuation"));
        }
        let point = self.opening_point();
        let actor = point["by"]
            .as_str()
            .and_then(super::pipeline::viewer)
            .ok_or_else(|| invalid("invalid opening actor"))?;
        let shape = match self.state.flow["stage"].as_str() {
            Some("amulet") => OpeningQuestion::StartAmulet(
                self.opening_cards(actor, &self.start_amulets(seat(actor))?)?,
            ),
            Some("first") => OpeningQuestion::FirstPlayer,
            Some("mulligan") => OpeningQuestion::Mulligan(
                self.opening_cards(actor, &self.zone_ids(seat(actor), "hand"))?,
            ),
            _ => return Err(invalid("invalid opening stage")),
        };
        Ok(Question {
            token: QuestionToken {
                epoch: self.state.ir_epoch,
                serial: self.player_epoch,
                action: true,
            },
            actor,
            purpose: QuestionPurpose::RuleChoice,
            shape: QuestionShape::Opening(shape),
        })
    }

    fn opening_cards(&self, actor: Viewer, ids: &[String]) -> Result<Vec<CardObservation>> {
        ids.iter().map(|id| self.observed_card(actor, id)).collect()
    }

    fn opening_payload(&self, actor: Viewer, answer: OpeningAnswer) -> Result<Value> {
        let mut payload = match (self.state.flow["stage"].as_str(), answer) {
            (Some("amulet"), OpeningAnswer::StartAmulet(handle)) => {
                let id = self.resolve_player_object(actor, &handle)?;
                if !self
                    .start_amulets(seat(actor))?
                    .iter()
                    .any(|candidate| candidate == id)
                {
                    return Err(invalid("ineligible opening amulet"));
                }
                json!({"do":"choose-start-amulet","object":id})
            }
            (Some("first"), OpeningAnswer::FirstPlayer(first)) => {
                json!({"do":"choose-first","first":seat(first)})
            }
            (Some("mulligan"), OpeningAnswer::Keep) => json!({"do":"mulligan","redo":false}),
            (Some("mulligan"), OpeningAnswer::Mulligan(handles)) => {
                let ids = handles
                    .iter()
                    .map(|handle| self.resolve_player_object(actor, handle))
                    .collect::<Result<Vec<_>>>()?;
                let unique = ids.iter().copied().collect::<BTreeSet<_>>();
                let hand = self.zone_ids(seat(actor), "hand");
                if ids.len() != hand.len()
                    || unique.len() != hand.len()
                    || hand.iter().any(|id| !unique.contains(id.as_str()))
                {
                    return Err(invalid("mulligan must order the entire hand"));
                }
                json!({"do":"mulligan","redo":true,"order":ids})
            }
            _ => return Err(invalid("answer belongs to another opening stage")),
        };
        payload["by"] = json!(seat(actor));
        Ok(payload)
    }

    pub(super) fn submit_player_opening(
        &mut self,
        actor: Viewer,
        token: QuestionToken,
        answer: OpeningAnswer,
    ) -> SubmissionKind {
        if self.player_fault.is_some() {
            return SubmissionKind::EngineFault;
        }
        if self.state.game["ended"] == true || self.state.flow["kind"] != "pregame" {
            return SubmissionKind::Rejected;
        }
        let input = self.input(actor);
        let super::Input::Question(question) = input else {
            return if matches!(input, super::Input::EngineFault) {
                SubmissionKind::EngineFault
            } else {
                SubmissionKind::Rejected
            };
        };
        if question.actor != actor || question.token != token {
            return SubmissionKind::Rejected;
        }
        let payload = match self.opening_payload(actor, answer) {
            Ok(payload) => payload,
            Err(EngineFailure::Invalid(_)) => return SubmissionKind::Rejected,
            Err(error) => {
                self.player_fault = Some(error);
                return SubmissionKind::EngineFault;
            }
        };
        let Some(next) = self.player_epoch.checked_add(1) else {
            self.player_fault = Some(invalid("player input epoch exhausted"));
            return SubmissionKind::EngineFault;
        };
        let before = self.clone();
        let published = self.published_knowledge();
        if let Err(error) = self.apply_opening_decision(&payload) {
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
