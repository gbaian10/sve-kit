#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use core::mem::take;
use serde_json::json;

use super::{Game, list, string};
use crate::{EngineFailure, Result, invalid};

impl Game {
    pub(super) fn import_cost_history(&mut self) -> Result<()> {
        let mut entries = Vec::new();
        for mut entry in take(&mut self.state.continuous) {
            if !entry["effect"].is_null() || !string(&entry["text"]).contains("コスト") {
                entries.push(entry);
                continue;
            }
            let source = string(&entry["source"]);
            let node = self.historical_effect(source, string(&entry["text"]), "adjust_cost")?;
            if ["amount", "set"]
                .iter()
                .any(|key| !node[*key].is_null() && !node[*key].is_i64())
            {
                return Err(EngineFailure::Unsupported(
                    "historical cost expression needs a saved context".into(),
                ));
            }
            let context = self.frame_for(source)?;
            let subjects = list(&entry["applies_to"]);
            entry["effect"] = node;
            entry["controller"] = json!(context.controller);
            entry["context"] = serde_json::to_value(context).map_err(invalid)?;
            entry["dynamic"] = json!(
                subjects
                    .iter()
                    .all(|id| self.state.players.contains_key(string(id)))
            );
            entry["generations"] = json!({});
            for id in &subjects {
                if let Some(object) = self.state.objects.get(string(id)) {
                    entry["generations"][string(id)] = json!(object.generation);
                }
            }
            entries.push(entry);
        }
        self.state.continuous = entries;
        Ok(())
    }
}
