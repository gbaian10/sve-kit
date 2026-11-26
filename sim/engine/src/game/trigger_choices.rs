#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use serde_json::Value;

use super::{Game, TriggerAlternative};
use crate::{EngineFailure, Result, invalid};

impl Game {
    pub(super) fn validate_trigger_choices(&self) -> Result<()> {
        if self.state.game["ended"] != true
            && self
                .state
                .pending
                .iter()
                .any(|pending| !pending.alternatives.is_empty())
            && (self.state.prompt.is_some() || self.state.pending.len() != 1)
        {
            return Err(EngineFailure::Unsupported(
                "a deferred trigger condition cannot cross another input or pending resolution"
                    .into(),
            ));
        }
        Ok(())
    }

    pub(super) fn import_trigger_alternatives(pending: &Value) -> Result<Vec<TriggerAlternative>> {
        let alternatives: Vec<TriggerAlternative> = pending.get("event_alternatives").map_or_else(
            || Ok(Vec::new()),
            |value| serde_json::from_value(value.clone()).map_err(invalid),
        )?;
        if pending["alternatives_required"] == true && alternatives.is_empty() {
            return Err(EngineFailure::Unsupported(
                "observer has no inspectable trigger alternatives".into(),
            ));
        }
        if !alternatives.is_empty() && (!pending["id"].is_null() || !pending["event"].is_null()) {
            return Err(invalid(
                "unselected trigger alternatives cannot have a selected event or pending id",
            ));
        }
        Ok(alternatives)
    }
}
