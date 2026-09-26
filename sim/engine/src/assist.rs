//! Sandbox and shadow snapshots with append-only divergence records.

#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]

use serde_json::{Value, json};

use crate::game::{Game, Step, View};
use crate::{Result, invalid};

/// The two coordinated state layers.
#[derive(Debug, Clone, Copy)]
pub enum Layer {
    /// Player-operated board.
    Sandbox,
    /// Rules-checked board.
    Shadow,
}

/// An operation recorded with a stable causal decision identity.
#[derive(Debug, Clone)]
pub struct Applied {
    /// Immutable decision identity.
    pub decision: String,
    /// Result from the authoritative core.
    pub step: Step,
}

/// Assisted play; unresolved divergence disables all automatic advancement.
#[derive(Debug, Clone)]
pub struct Assist {
    sandbox: Game,
    shadow: Game,
    records: Vec<Value>,
    operations: Vec<(Applied, Applied)>,
    next: u64,
}

impl Assist {
    /// Opens two layers from one snapshot.
    #[must_use]
    pub fn new(game: Game) -> Self {
        Self {
            sandbox: game.clone(),
            shadow: game,
            records: Vec::new(),
            operations: Vec::new(),
            next: 1,
        }
    }

    /// Applies an ordinary decision, or one explicitly manual atomic operation.
    ///
    /// # Errors
    /// The core cannot execute or represent the operation.
    pub fn act(&mut self, operation: &Value) -> Result<(Applied, Applied)> {
        let id = format!("assist:n{}", self.next);
        self.next = self.next.saturating_add(1);
        let before = self.shadow.projection(View::Referee)?;
        let expected = before["awaiting"].clone();
        let (sandbox, shadow) = if operation["manual"] == true {
            let sandbox = self.sandbox.manual(operation, &id)?;
            let shadow = Step {
                outcome: "cannot-play".into(),
                events: Vec::new(),
            };
            let record = json!({"id":format!("divergence-{}",self.records.len().saturating_add(1)),"kind":if operation["do"]=="set" {"mismatch"} else {"missed"},"expected":if operation["do"]=="set" {Game::packet_query(&before,operation["path"].as_str().unwrap_or_default()).unwrap_or(Value::Null)} else {expected},"actual":operation,"refs":["7.3.4","10.7.3.1"],"resolved":null});
            self.records.push(record);
            (sandbox, shadow)
        } else {
            let shadow = self.shadow.decide(operation, &id)?;
            let sandbox = self.sandbox.decide(operation, &id)?;
            (sandbox, shadow)
        };
        let pair = (
            Applied {
                decision: id.clone(),
                step: sandbox,
            },
            Applied {
                decision: id,
                step: shadow,
            },
        );
        self.operations.push(pair.clone());
        Ok(pair)
    }

    /// Whether automation and AI may advance.
    #[must_use]
    pub fn auto_advance(&self) -> bool {
        self.records
            .iter()
            .all(|record| !record["resolved"].is_null())
    }

    /// Stops at unresolved divergences or at a player input; automatic rule steps already ran.
    #[must_use]
    pub fn advance(&self) -> Option<(Applied, Applied)> {
        if !self.auto_advance() {
            return None;
        }
        None
    }

    /// Resolves a divergence by rewinding the board, or adopting the manual state.
    ///
    /// # Errors
    /// The record does not exist or is already resolved.
    pub fn realign(&mut self, id: &str, adopt: bool) -> Result<()> {
        let record = self
            .records
            .iter_mut()
            .find(|r| r["id"] == id)
            .ok_or_else(|| invalid("unknown divergence"))?;
        if !record["resolved"].is_null() {
            return Err(invalid("divergence already resolved"));
        }
        if adopt {
            self.shadow = self.sandbox.clone();
        } else {
            self.sandbox = self.shadow.clone();
        }
        record["resolved"] = json!(if adopt { "adopt" } else { "rewind" });
        Ok(())
    }

    /// All divergence records, including resolved ones.
    #[must_use]
    pub fn divergences(&self) -> Vec<Value> {
        self.records.clone()
    }

    /// One layer's authoritative state.
    #[must_use]
    pub const fn game(&self, layer: Layer) -> &Game {
        match layer {
            Layer::Sandbox => &self.sandbox,
            Layer::Shadow => &self.shadow,
        }
    }
}
