#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::sync::Arc;

use serde_json::Value;
use sve_scenario_runner::arch::{
    Applied, AssistEngine, BranchKind, Layer, Layered, NodeId, Observation, Realign, ReplayEngine,
};
use sve_scenario_runner::{EngineError, Fixture, Step, View};

use super::{core_view, error};
use crate::assist::{self, Assist};
use crate::catalog::Catalog;
use crate::game::{Game, Step as CoreStep};
use crate::replay::Replay;

fn step(step: CoreStep) -> Step {
    Step {
        outcome: step.outcome,
        events: step.events,
    }
}
fn observe(game: &Game, view: View) -> Result<Observation, EngineError> {
    let projection = game.projection(core_view(view)).map_err(error)?;
    let awaiting = projection.get("awaiting").filter(|v| !v.is_null()).cloned();
    let knowledge = projection["knowledge"].clone();
    Ok(Observation {
        projection,
        awaiting,
        knowledge,
    })
}

/// Format-only adapter for replay architecture checks.
#[derive(Debug, Clone)]
pub struct ReplayAdapter {
    catalog: Arc<Catalog>,
    replay: Replay,
}
impl ReplayAdapter {
    /// Uses the same validated catalogue as ordinary matches.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>) -> Self {
        Self {
            catalog,
            replay: Replay::default(),
        }
    }
}
impl ReplayEngine for ReplayAdapter {
    fn start(&mut self, fixture: &Fixture, seed: &str, game: &str) -> Result<NodeId, EngineError> {
        let initial = Game::new(
            Arc::clone(&self.catalog),
            &fixture.setup,
            &fixture.card_facts,
            &fixture.random,
            seed,
        )
        .map_err(error)?;
        let (replay, id) = Replay::start(initial, game);
        self.replay = replay;
        Ok(NodeId(id))
    }
    fn decide(&mut self, at: &NodeId, decision: &Value) -> Result<(NodeId, Step), EngineError> {
        let (id, result) = self.replay.decide(&at.0, decision).map_err(error)?;
        Ok((NodeId(id), step(result)))
    }
    fn branch(&mut self, at: &NodeId, kind: BranchKind) -> Result<NodeId, EngineError> {
        self.replay
            .branch(&at.0, matches!(kind, BranchKind::Undo))
            .map(NodeId)
            .map_err(error)
    }
    fn save(&self, at: &NodeId) -> Result<Vec<u8>, EngineError> {
        self.replay.save(&at.0).map_err(error)
    }
    fn restore(&mut self, blob: &[u8]) -> Result<NodeId, EngineError> {
        let (replay, id) = Replay::restore(blob).map_err(error)?;
        self.replay = replay;
        Ok(NodeId(id))
    }
    fn export(&self, at: &NodeId, view: View) -> Result<Value, EngineError> {
        self.replay
            .game(&at.0)
            .map_err(error)?
            .projection(core_view(view))
            .map_err(error)
    }
    fn digest(&self, at: &NodeId) -> Result<String, EngineError> {
        self.replay
            .game(&at.0)
            .map_err(error)?
            .digest()
            .map_err(error)
    }
    fn observe(&self, at: &NodeId, view: View) -> Result<Observation, EngineError> {
        observe(self.replay.game(&at.0).map_err(error)?, view)
    }
    fn query(&self, at: &NodeId, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        self.replay
            .game(&at.0)
            .map_err(error)?
            .query(core_view(view), path)
            .map_err(error)
    }
    fn decisions(&self, at: &NodeId) -> Result<Vec<Value>, EngineError> {
        self.replay.decisions(&at.0).map_err(error)
    }
    fn events(&self, at: &NodeId) -> Result<Vec<Value>, EngineError> {
        self.replay.events(&at.0).map_err(error)
    }
    fn admin_log(&self) -> Result<Vec<Value>, EngineError> {
        Ok(self.replay.admin_log())
    }
}

/// Format-only adapter for two-layer assisted play.
#[derive(Debug, Clone)]
pub struct AssistAdapter {
    catalog: Arc<Catalog>,
    assist: Option<Assist>,
}
impl AssistAdapter {
    /// Uses the same validated catalogue as ordinary matches.
    #[must_use]
    pub const fn new(catalog: Arc<Catalog>) -> Self {
        Self {
            catalog,
            assist: None,
        }
    }
    fn assist(&self) -> Result<&Assist, EngineError> {
        self.assist
            .as_ref()
            .ok_or_else(|| EngineError::Adapter("assist not started".into()))
    }
    fn assist_mut(&mut self) -> Result<&mut Assist, EngineError> {
        self.assist
            .as_mut()
            .ok_or_else(|| EngineError::Adapter("assist not started".into()))
    }
}
const fn core_layer(layer: Layer) -> assist::Layer {
    match layer {
        Layer::Sandbox => assist::Layer::Sandbox,
        Layer::Shadow => assist::Layer::Shadow,
    }
}
fn applied(applied: assist::Applied) -> Applied {
    Applied {
        decision: applied.decision,
        step: step(applied.step),
    }
}
impl AssistEngine for AssistAdapter {
    fn start(&mut self, fixture: &Fixture, seed: &str, _game: &str) -> Result<(), EngineError> {
        self.assist = Some(Assist::new(
            Game::new(
                Arc::clone(&self.catalog),
                &fixture.setup,
                &fixture.card_facts,
                &fixture.random,
                seed,
            )
            .map_err(error)?,
        ));
        Ok(())
    }
    fn act(&mut self, op: &Value) -> Result<Layered<Applied>, EngineError> {
        let (sandbox, shadow) = self.assist_mut()?.act(op).map_err(error)?;
        Ok(Layered {
            sandbox: applied(sandbox),
            shadow: applied(shadow),
        })
    }
    fn divergences(&self) -> Result<Vec<Value>, EngineError> {
        Ok(self.assist()?.divergences())
    }
    fn auto_advance(&self) -> Result<bool, EngineError> {
        Ok(self.assist()?.auto_advance())
    }
    fn advance(&mut self) -> Result<Option<Layered<Applied>>, EngineError> {
        Ok(self.assist()?.advance().map(|(sandbox, shadow)| Layered {
            sandbox: applied(sandbox),
            shadow: applied(shadow),
        }))
    }
    fn realign(&mut self, divergence: &str, mode: Realign) -> Result<(), EngineError> {
        self.assist_mut()?
            .realign(divergence, matches!(mode, Realign::Adopt))
            .map_err(error)
    }
    fn digest(&self, layer: Layer) -> Result<String, EngineError> {
        self.assist()?
            .game(core_layer(layer))
            .digest()
            .map_err(error)
    }
    fn observe(&self, layer: Layer, view: View) -> Result<Observation, EngineError> {
        observe(self.assist()?.game(core_layer(layer)), view)
    }
    fn query(&self, layer: Layer, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        self.assist()?
            .game(core_layer(layer))
            .query(core_view(view), path)
            .map_err(error)
    }
}
