//! Trusted typed exam runner. Seats act only through the player interface; the referee
//! builds the game, resolves fixture labels to capabilities, and reads authority state
//! and raw rules events for checkpoint comparison.

#![expect(
    clippy::indexing_slicing,
    reason = "Exam decisions are read through serde_json's total indexing; a missing key is null."
)]

mod labels;
mod params;
mod turn;

use core::fmt::Display;
use serde_json::{Value, json};
use sve_scenario_runner::{Engine, EngineError, Fixture, Step, View};

use super::{core_view, error};
use crate::catalog::SelectedEffects;
use crate::game::Game;
use crate::game::player::{Input, QuestionShape, Viewer};
use labels::Labels;
use turn::{Turn, act, settle};

/// Exam adapter over the typed player entry with referee-only authority reads.
#[derive(Debug, Clone)]
pub struct TypedAdapter {
    selected: SelectedEffects,
    game: Option<Game>,
    setup: Value,
}

impl TypedAdapter {
    /// Uses an accepted current selection; each load builds an independent game.
    #[must_use]
    pub const fn new(selected: SelectedEffects) -> Self {
        Self {
            selected,
            game: None,
            setup: Value::Null,
        }
    }

    /// The exam's pending reference as `{ability, event}`, following a setup id.
    fn pending_reference(&self, pending: &Value) -> Value {
        if let Some(id) = pending.as_str() {
            return self.setup["semantic_state"]["pending_triggers"]
                .as_array()
                .and_then(|items| items.iter().find(|item| item["id"] == id))
                .cloned()
                .unwrap_or(Value::Null);
        }
        pending.clone()
    }

    fn game(&self) -> Result<&Game, EngineError> {
        self.game
            .as_ref()
            .ok_or_else(|| EngineError::Adapter("game not loaded".into()))
    }
}

fn adapter(message: impl Into<String>) -> EngineError {
    EngineError::Adapter(message.into())
}

fn untranslated(what: impl Display) -> EngineError {
    EngineError::Unsupported(format!("typed runner: {what}"))
}

fn fault(game: &Game) -> EngineError {
    game.referee_fault().map_or_else(
        || adapter("player progression stopped without a diagnostic"),
        |failure| error(failure.clone()),
    )
}

fn seat(value: &Value) -> Result<Viewer, EngineError> {
    match value.as_str() {
        Some("P1") => Ok(Viewer::P1),
        Some("P2") => Ok(Viewer::P2),
        _ => Err(adapter(format!("invalid seat {value}"))),
    }
}

fn shape_name(shape: &QuestionShape) -> String {
    match shape {
        QuestionShape::Decision(decision) => {
            let text = format!("{decision:?}");
            text.split([' ', '(', '{'])
                .next()
                .unwrap_or_default()
                .to_owned()
        }
        QuestionShape::Evolution(_) => "Evolution".into(),
        QuestionShape::Opening(_) => "Opening".into(),
        QuestionShape::Closing(_) => "Closing".into(),
        QuestionShape::ActionWindow(_) => "ActionWindow".into(),
    }
}

impl Engine for TypedAdapter {
    fn load(&mut self, fixture: &Fixture) -> Result<(), EngineError> {
        let mut game = Game::from_selected_effects(
            self.selected.clone(),
            &fixture.setup,
            &fixture.card_facts,
            &fixture.random,
            "g1",
        )
        .map_err(error)?;
        game.referee_record_events();
        game.advance_player_input();
        if game.referee_fault().is_some() {
            return Err(fault(&game));
        }
        self.setup = fixture.setup.clone();
        self.game = Some(game);
        Ok(())
    }

    fn decide(&mut self, decision: &Value) -> Result<Step, EngineError> {
        let by = seat(&decision["by"])?;
        let pending = self.pending_reference(&decision["pending"]);
        let game = self
            .game
            .as_mut()
            .ok_or_else(|| adapter("game not loaded"))?;
        settle(game, decision, by)?;
        let outcome = act(&mut Turn { game, by }, decision, &pending)?;
        Ok(Step {
            outcome,
            events: game.referee_take_events(),
        })
    }

    fn query(&self, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        self.game()?.query(core_view(view), path).map_err(error)
    }

    fn awaiting(&self, view: View) -> Result<Option<Value>, EngineError> {
        let game = self
            .game
            .as_ref()
            .ok_or_else(|| adapter("game not loaded"))?;
        for seat in [Viewer::P1, Viewer::P2] {
            let Input::Question(question) = game.input(seat) else {
                continue;
            };
            let by = format!("{seat:?}");
            let own = match view {
                View::Omniscient => true,
                View::P1 => seat == Viewer::P1,
                View::P2 => seat == Viewer::P2,
            };
            if !own {
                return Ok(Some(json!({"by": by})));
            }
            let labels = Labels { game, by: seat };
            return Ok(Some(labels.choices(&question).map_or_else(
                || json!({"by": by, "unrendered": shape_name(&question.shape)}),
                |choices| {
                    json!({"by": by, "choices": choices, "fields": Labels::fields(&question)})
                },
            )));
        }
        Ok(None)
    }

    fn projection(&self, view: View) -> Result<Value, EngineError> {
        self.game()?.projection(core_view(view)).map_err(error)
    }
}
