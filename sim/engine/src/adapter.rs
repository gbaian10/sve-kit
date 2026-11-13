//! Thin neutral-runner adapter; it only converts input and result types.

mod architecture;
mod intelligence;
mod typed;
pub use architecture::{AssistAdapter, ReplayAdapter};
pub use intelligence::AiAdapter;
pub use typed::TypedAdapter;

use alloc::sync::Arc;

use serde_json::Value;
use sve_scenario_runner::{Engine, EngineError, Fixture, Step, View};

use crate::catalog::Catalog;
use crate::game::{self, Game};

/// Adapter to the shared rule-scenario contract.
#[derive(Debug, Clone)]
pub struct Adapter {
    catalog: Arc<Catalog>,
    game: Option<Game>,
}

impl Adapter {
    /// Uses an already schema-validated immutable catalogue.
    #[must_use]
    pub const fn new(catalog: Arc<Catalog>) -> Self {
        Self {
            catalog,
            game: None,
        }
    }
    fn game(&self) -> Result<&Game, EngineError> {
        self.game
            .as_ref()
            .ok_or_else(|| EngineError::Adapter("game not loaded".into()))
    }
}

pub(crate) fn error(error: crate::EngineFailure) -> EngineError {
    match error {
        crate::EngineFailure::Unsupported(reason) => EngineError::Unsupported(reason),
        crate::EngineFailure::Invalid(reason)
        | crate::EngineFailure::IllegalPlay(reason)
        | crate::EngineFailure::AuthorizationIntegrity(reason)
        | crate::EngineFailure::Invariant(reason) => EngineError::Adapter(reason),
    }
}
pub(crate) const fn core_view(view: View) -> game::View {
    match view {
        View::Omniscient => game::View::Referee,
        View::P1 => game::View::P1,
        View::P2 => game::View::P2,
    }
}

impl Engine for Adapter {
    fn load(&mut self, fixture: &Fixture) -> Result<(), EngineError> {
        self.game = Some(
            Game::new(
                Arc::clone(&self.catalog),
                &fixture.setup,
                &fixture.card_facts,
                &fixture.random,
                "g1",
            )
            .map_err(error)?,
        );
        Ok(())
    }
    fn decide(&mut self, decision: &Value) -> Result<Step, EngineError> {
        let game = self
            .game
            .as_mut()
            .ok_or_else(|| EngineError::Adapter("game not loaded".into()))?;
        let step = game.submit(decision).map_err(error)?;
        Ok(Step {
            outcome: step.outcome,
            events: step.events,
        })
    }
    fn query(&self, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        self.game()?.query(core_view(view), path).map_err(error)
    }
    fn awaiting(&self, view: View) -> Result<Option<Value>, EngineError> {
        Ok(self
            .projection(view)?
            .get("awaiting")
            .filter(|v| !v.is_null())
            .cloned())
    }
    fn projection(&self, view: View) -> Result<Value, EngineError> {
        self.game()?.projection(core_view(view)).map_err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::error;
    use crate::EngineFailure;
    use sve_scenario_runner::EngineError;

    #[test]
    fn only_capability_gaps_are_reported_as_unsupported_to_the_runner() {
        let reason = "synthetic diagnostic";
        assert!(matches!(
            error(EngineFailure::Unsupported(reason.into())),
            EngineError::Unsupported(message) if message == reason
        ));
        for failure in [
            EngineFailure::Invalid(reason.into()),
            EngineFailure::IllegalPlay(reason.into()),
            EngineFailure::AuthorizationIntegrity(reason.into()),
            EngineFailure::Invariant(reason.into()),
        ] {
            assert!(matches!(error(failure), EngineError::Adapter(message) if message == reason));
        }
    }
}
