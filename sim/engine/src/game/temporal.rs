#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use super::{Game, int};
use crate::Result;
use crate::ir::SnapshotId;
use serde_json::{Value, json};

impl Game {
    pub(super) fn life_change_triggers(
        &mut self,
        changes: &[(String, i64, i64)],
        cause: &Value,
        battle: bool,
        snapshot: SnapshotId,
    ) -> Result<()> {
        if changes.iter().any(|(_, before, after)| before != after) {
            self.require_ability_sources("event leader_life_change subscriptions")?;
        }
        self.internal_life_change_events(changes, cause, battle, snapshot)?;
        self.group();
        Ok(())
    }

    pub(super) fn change_life(&mut self, seat: &str, life: i64) -> Result<()> {
        let previous = int(&self.player(seat)?.leader["life"]);
        self.player_mut(seat)?.leader["life"] = json!(life);
        self.internal_record_life_history(seat, previous, life);
        Ok(())
    }
}
