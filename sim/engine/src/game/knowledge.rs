use super::Game;
use alloc::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Knowledge {
    pub(super) seen: BTreeMap<String, Value>,
    located: BTreeSet<String>,
    pub(super) carried: Vec<Value>,
}

impl Knowledge {
    pub(super) const fn located(&self) -> &BTreeSet<String> {
        &self.located
    }

    pub(super) fn locate(&mut self, id: &str) {
        self.located.insert(id.into());
    }

    pub(super) fn forget_location(&mut self, id: &str) {
        self.located.remove(id);
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn learn(&mut self, seat: &str, id: &str, located: bool) {
        let Some(object) = self.state.objects.get(id) else {
            return;
        };
        let knowledge = self.state.knowledge.entry(seat.into()).or_default();
        knowledge.seen.entry(id.into()).or_insert_with(
            || json!({"id":id,"card":object.card,"owner":object.owner,"from":self.node}),
        );
        if located {
            knowledge.locate(id);
        }
    }
}
