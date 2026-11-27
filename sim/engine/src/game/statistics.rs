#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use serde_json::json;

use super::{Game, Object, int};
use crate::Result;

impl Game {
    pub(super) fn record_evolution_stat_change(&mut self, previous: &Object) -> Result<()> {
        for field in ["power", "hp"] {
            let old = int(&previous.state[field]);
            let new = int(&self.object(&previous.id)?.state[field]);
            if old == new {
                continue;
            }
            if new > old {
                self.object_mut(&previous.id)?.state["stats_increased_this_turn"] = json!(true);
            }
            let direction = if new > old { "increase" } else { "decrease" };
            self.require_ability_sources(&format!("event {field}_{direction} subscriptions"))?;
        }
        Ok(())
    }
}
