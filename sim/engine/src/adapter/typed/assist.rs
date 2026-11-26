//! Neutral assist exam over the typed authority's branches and seat views.

use super::{adapter, decide_on, observe_on, seat, untranslated};
use crate::adapter::error;
use crate::catalog::SelectedEffects;
use crate::game::Game;
use crate::game::assist::{self, Assist, Divergence, Expected, ManualOverride};
use crate::game::player::{AttackTarget, ObjectHandle, Viewer, observation::ObservedZone};
use crate::game::replay::Replay;
use serde_json::{Value, json};
use sve_scenario_runner::arch::{Applied, AssistEngine, Layer, Layered, Observation, Realign};
use sve_scenario_runner::{EngineError, Fixture, Step, View};

/// Runs the architecture fixture vocabulary against the assisted typed game.
#[derive(Debug)]
pub struct TypedAssistAdapter {
    selected: SelectedEffects,
    assist: Option<Assist>,
}

impl TypedAssistAdapter {
    /// Each fixture starts an independent assisted game with these admitted effects.
    #[must_use]
    pub const fn new(selected: SelectedEffects) -> Self {
        Self {
            selected,
            assist: None,
        }
    }

    fn assist(&self) -> Result<&Assist, EngineError> {
        self.assist
            .as_ref()
            .ok_or_else(|| adapter("assist not started"))
    }

    fn game(&self, layer: Layer) -> Result<&Game, EngineError> {
        let assist = self.assist()?;
        assist
            .replay
            .game(assist.node(core_layer(layer)))
            .map_err(error)
    }
}

impl AssistEngine for TypedAssistAdapter {
    fn start(&mut self, fixture: &Fixture, seed: &str, _game: &str) -> Result<(), EngineError> {
        let replay = Replay::from_position(
            self.selected.clone(),
            fixture.setup.clone(),
            fixture.card_facts.clone(),
            fixture.random.clone(),
            seed.into(),
        )
        .map_err(error)?;
        self.assist = Some(Assist::new(replay).map_err(error)?);
        Ok(())
    }

    fn act(&mut self, op: &Value) -> Result<Layered<Applied>, EngineError> {
        let assist = self
            .assist
            .as_mut()
            .ok_or_else(|| adapter("assist not started"))?;
        if op.get("manual") == Some(&Value::Bool(true)) {
            return manual(assist, op);
        }
        let step = assist.advance(error, |game, commands| decide_on(game, op, Some(commands)))?;
        let applied = Applied {
            decision: assist.node(assist::Layer::Shadow).to_string(),
            step,
        };
        Ok(Layered {
            sandbox: applied.clone(),
            shadow: applied,
        })
    }

    fn divergences(&self) -> Result<Vec<Value>, EngineError> {
        let assist = self.assist()?;
        let mut records: Vec<_> = [Viewer::P1, Viewer::P2]
            .into_iter()
            .flat_map(|viewer| assist.divergences(viewer))
            .collect();
        records.sort_by_key(|record| record.node);
        records
            .into_iter()
            .map(|record| render(assist, record))
            .collect()
    }

    fn auto_advance(&self) -> Result<bool, EngineError> {
        Ok(self.assist()?.auto_advance())
    }

    fn advance(&mut self) -> Result<Option<Layered<Applied>>, EngineError> {
        // Typed submissions already advance deterministic rules to the next player input.
        self.assist()?;
        Ok(None)
    }

    fn realign(&mut self, divergence: &str, mode: Realign) -> Result<(), EngineError> {
        let assist = self
            .assist
            .as_mut()
            .ok_or_else(|| adapter("assist not started"))?;
        let record = assist
            .pending()
            .filter(|record| record.node.to_string() == divergence)
            .ok_or_else(|| adapter("unknown pending divergence"))?;
        let mode = match mode {
            Realign::Rewind => assist::Realign::Rewind,
            Realign::Adopt => assist::Realign::Adopt,
        };
        assist
            .realign(record.viewer, record.node, mode)
            .map_err(error)
    }

    fn digest(&self, layer: Layer) -> Result<String, EngineError> {
        self.game(layer)?.referee_replay_digest().map_err(error)
    }

    fn observe(&self, layer: Layer, view: View) -> Result<Observation, EngineError> {
        observe_on(self.game(layer)?, view)
    }

    fn query(&self, layer: Layer, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        Ok(Game::packet_query(
            &self.observe(layer, view)?.projection,
            path,
        ))
    }
}

const fn core_layer(layer: Layer) -> assist::Layer {
    match layer {
        Layer::Sandbox => assist::Layer::Sandbox,
        Layer::Shadow => assist::Layer::Shadow,
    }
}

fn manual(assist: &mut Assist, op: &Value) -> Result<Layered<Applied>, EngineError> {
    let viewer = seat(op.get("by").unwrap_or(&Value::Null))?;
    let at = assist.node(assist::Layer::Sandbox);
    let command = parse_override(assist.replay.game(at).map_err(error)?, viewer, op)?;
    let node = assist.manual_override(viewer, command).map_err(error)?;
    let shadow = match assist.pending().map(|record| &record.expected) {
        Some(Expected::Attack { allowed: false, .. }) => "cannot-attack",
        Some(_) => "override",
        None => "resolved",
    };
    let applied = |decision, outcome: &str| Applied {
        decision,
        step: Step {
            outcome: outcome.into(),
            events: Vec::new(),
        },
    };
    Ok(Layered {
        sandbox: applied(node.unwrap_or(at).to_string(), "resolved"),
        shadow: applied(at.to_string(), shadow),
    })
}

fn parse_override(game: &Game, viewer: Viewer, op: &Value) -> Result<ManualOverride, EngineError> {
    match op.get("do").and_then(Value::as_str) {
        Some("attack") => {
            let attacker = field_handle(game, viewer, op.get("attacker").and_then(Value::as_str))?;
            let label = op.get("target").and_then(Value::as_str);
            let target = match label.and_then(|text| text.strip_suffix(".leader")) {
                Some(player) => AttackTarget::Leader(seat(&json!(player))?),
                None => AttackTarget::Card(field_handle(game, viewer, label)?),
            };
            Ok(ManualOverride::MarkAttack { attacker, target })
        }
        Some("set") => {
            let player = op
                .get("path")
                .and_then(Value::as_str)
                .and_then(|path| path.strip_suffix(".leader.life"))
                .ok_or_else(|| untranslated("manual property"))?;
            let life = op
                .get("value")
                .and_then(Value::as_i64)
                .ok_or_else(|| adapter("invalid manual life"))?;
            Ok(ManualOverride::SetLife {
                player: seat(&json!(player))?,
                life,
            })
        }
        _ => Err(untranslated("manual operation")),
    }
}

fn field_handle(
    game: &Game,
    viewer: Viewer,
    label: Option<&str>,
) -> Result<ObjectHandle, EngineError> {
    let wanted = label.ok_or_else(|| adapter("manual attack requires a visible field card"))?;
    game.observe(viewer)
        .map_err(error)?
        .players
        .into_iter()
        .flat_map(|player| player.zones)
        .filter(|zone| zone.kind == ObservedZone::Field)
        .flat_map(|zone| zone.cards)
        .map(|entry| entry.card.handle)
        .find(|handle| game.referee_label(viewer, handle).as_deref() == Some(wanted))
        .ok_or_else(|| adapter("manual attack requires a visible field card"))
}

fn render(assist: &Assist, record: &Divergence) -> Result<Value, EngineError> {
    let game = assist.replay.game(record.before).map_err(error)?;
    let label = |handle| game.referee_label(record.viewer, handle);
    let (kind, expected) = match &record.expected {
        Expected::LeaderLife(life) => ("mismatch", json!(life)),
        Expected::Attack { allowed, pending } => {
            let kind = if *allowed {
                "mismatch"
            } else if pending.is_empty() {
                "illegal"
            } else {
                "missed"
            };
            let choices: Vec<_> = pending
                .iter()
                .map(|choice| {
                    let source = choice
                        .source
                        .as_ref()
                        .and_then(|source| source.holder.as_ref())
                        .and_then(label);
                    json!({"pending": {"ability": {"source": source, "line": choice.line}}})
                })
                .collect();
            (kind, json!({"allowed": allowed, "choices": choices}))
        }
    };
    let by = format!("{:?}", record.viewer);
    let actual = match &record.actual {
        ManualOverride::SetLife { player, life } => {
            json!({"by": by, "do": "set", "path": format!("{player:?}.leader.life"), "value": life, "manual": true})
        }
        ManualOverride::MarkAttack { attacker, target } => {
            let target = match target {
                AttackTarget::Leader(player) => json!(format!("{player:?}.leader")),
                AttackTarget::Card(handle) => json!(label(handle)),
            };
            json!({"by": by, "do": "attack", "attacker": label(attacker), "target": target, "manual": true})
        }
    };
    let resolved = record.resolved.map(|mode| match mode {
        assist::Realign::Rewind => "rewind",
        assist::Realign::Adopt => "adopt",
    });
    Ok(json!({
        "id": record.node.to_string(),
        "node": record.before.to_string(),
        "kind": kind,
        "expected": expected,
        "actual": actual,
        "refs": record.expected.rule_refs(),
        "resolved": resolved
    }))
}
