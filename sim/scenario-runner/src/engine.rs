//! What an engine must provide to be run against the scenarios.
//!
//! The trait asks only for externally observable behaviour. How the neutral setup
//! becomes internal state is the adapter's business; the adapter may convert formats
//! but must not resolve rules or filter hidden information on the engine's behalf.

use serde_json::Value;

use crate::inherit::Fixture;

/// Whose view a checkpoint is observed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Full state, as the referee sees it.
    Omniscient,
    /// Player 1's projection.
    P1,
    /// Player 2's projection.
    P2,
}

impl View {
    /// Parses the contract spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "omniscient" => Some(Self::Omniscient),
            "P1" => Some(Self::P1),
            "P2" => Some(Self::P2),
            _ => None,
        }
    }
}

/// What happened after one decision (contract 6.2).
#[derive(Debug, Clone)]
pub struct Step {
    /// `resolved`, `paused`, `cannot-play`, `cannot-activate`, `cannot-evolve`,
    /// `cannot-attack`, `pending-cancelled` or `game-end`.
    pub outcome: String,
    /// Events produced while the engine advanced to the next input point, in order.
    ///
    /// Each event is a contract-shaped object (`{kind, ...}`). Events that happen
    /// simultaneously carry the same `group` value; the label itself is up to the engine.
    pub events: Vec<Value>,
}

/// Why an engine could not run a scenario. Kept apart from rule failures in reports.
#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    /// The prototype does not implement something the scenario needs.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// The adapter could not translate the scenario or a decision.
    #[error("adapter: {0}")]
    Adapter(String),
}

/// An engine under test, wrapped by its adapter.
pub trait Engine {
    /// Builds the position described by the fixture, replacing any previous state.
    ///
    /// # Errors
    /// When the adapter cannot build the position.
    fn load(&mut self, fixture: &Fixture) -> Result<(), EngineError>;

    /// Submits one decision (a contract-shaped object) and advances to the next
    /// input point or the end of the game.
    ///
    /// # Errors
    /// When the adapter cannot submit the decision. An illegal decision is not an
    /// error: it is a `cannot-*` outcome with the position rolled back.
    fn decide(&mut self, decision: &Value) -> Result<Step, EngineError>;

    /// The value at an `assert` path (contract 6.3, 9.12) as seen from `view`,
    /// or `None` when the path does not exist.
    ///
    /// Zone lists report unidentified cards as `{"filler": n}`. The path
    /// `knowledge` returns `{"identifiable": [ids]}`: every object the viewer can
    /// identify at this point.
    ///
    /// # Errors
    /// When the adapter cannot answer the query.
    fn query(&self, view: View, path: &str) -> Result<Option<Value>, EngineError>;

    /// The decision the engine is waiting for, as `view` sees it:
    /// `{"by": "P1", "choices": [...]}`, or `None` when the game has ended.
    ///
    /// When `view` is the other player, only `by` may be visible: the choices can
    /// be private (for example cards only `by` has looked at).
    ///
    /// # Errors
    /// When the adapter cannot describe the pending decision.
    fn awaiting(&self, view: View) -> Result<Option<Value>, EngineError>;

    /// Everything the engine would send to `view` at this point, in any shape.
    ///
    /// The runner scans it for object ids that `knowledge.does_not_know` says the
    /// viewer cannot identify: a leak anywhere in the projection is a failure, not
    /// only in the fields an assertion happens to name.
    ///
    /// # Errors
    /// When the adapter cannot produce the projection.
    fn projection(&self, view: View) -> Result<Value, EngineError>;
}
