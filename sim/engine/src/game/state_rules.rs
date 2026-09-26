#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use serde_json::json;

use super::{Frame, Game, int};
use crate::{EngineFailure, Result};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "State-based cleanup batches share the authoritative game state."
)]
impl Game {
    pub(super) fn clean_field_rules(&mut self) -> Result<bool> {
        let mut previous = Vec::new();
        let mut rules = json!({});
        let mut destroyed = Vec::new();
        for id in self.field_ids() {
            let object = self.object(&id)?.clone();
            let empty_stack = self.keywords(&id)?.contains("stack")
                && int(&object.state["counters"]["stack_counter"]) <= 0;
            let depleted = int(&object.state["hp"]) <= 0;
            let bane = object.state["bane_damaged"] == true;
            let destroy = self.object_type(&id)?.contains("フォロワー")
                && (depleted || (bane && !self.restricted(&id, "ability_destroy")?));
            if bane {
                self.object_mut(&id)?.state["bane_damaged"] = json!(false);
            }
            if destroy {
                destroyed.push((id.clone(), if depleted { "11.3.1" } else { "11.3.2" }));
            }
            if empty_stack || destroy {
                rules[&id] = json!(if empty_stack {
                    "11.7.1"
                } else if depleted {
                    "11.3.1"
                } else {
                    "11.3.2"
                });
                previous.push(object);
            }
        }
        if previous.is_empty() {
            return Ok(false);
        }
        let group = self.group();
        for (id, rule) in destroyed {
            self.emit(
                json!({"kind":"破壊","object":id}),
                &json!({"rule":rule}),
                group,
            );
        }
        let mut frame = Frame {
            cause: json!({"rule":"11"}),
            ..Frame::default()
        };
        frame.values.insert("movement_rules".into(), rules);
        let ids = previous
            .iter()
            .map(|object| object.id.clone())
            .collect::<Vec<_>>();
        self.move_objects(&ids, "cemetery", None, None, &frame)?;
        if previous.iter().all(|before| {
            self.state.objects.get(&before.id).is_some_and(|after| {
                after.zone == before.zone
                    && after.generation == before.generation
                    && after.state["counters"] == before.state["counters"]
            })
        }) {
            return Err(EngineFailure::Unsupported(
                "state-based field cleanup made no progress".into(),
            ));
        }
        Ok(true)
    }
}
