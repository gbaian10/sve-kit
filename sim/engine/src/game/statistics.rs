#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use serde_json::{Value, json};

use super::{Game, Object, Pending, int};
use crate::Result;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Stat events and their private contexts share the authoritative game state."
)]
impl Game {
    pub(super) fn stat_change_triggers(
        &mut self,
        previous: &[Object],
        cause: &Value,
    ) -> Result<Vec<Pending>> {
        let mut pending = Vec::new();
        for before in previous {
            for field in ["power", "hp"] {
                let old = int(&before.state[field]);
                let new = int(&self.object(&before.id)?.state[field]);
                if old == new {
                    continue;
                }
                if new > old {
                    self.object_mut(&before.id)?.state["stats_increased_this_turn"] = json!(true);
                }
                let event = format!(
                    "{field}_{}",
                    if new > old { "increase" } else { "decrease" }
                );
                let subject = self.event_subject(&before.id)?;
                pending.extend(self.collect_subject_event(
                    &event,
                    &[subject],
                    cause,
                    &json!({"stat":field,"before":old,"after":new}),
                )?);
            }
        }
        Ok(pending)
    }

    pub(super) fn disambiguate_unkeyed_pending(&mut self, pending: &mut Pending) {
        if pending.id.is_some() || !pending.event.is_null() {
            return;
        }
        let related = self
            .state
            .pending
            .iter()
            .enumerate()
            .filter(|(_, other)| {
                other.event.is_null()
                    && other.controller == pending.controller
                    && other.reference == pending.reference
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if related.iter().all(|index| {
            let other = &self.state.pending[*index];
            other.context == pending.context && other.code == pending.code
        }) {
            if let Some(index) = related.first() {
                pending.id.clone_from(&self.state.pending[*index].id);
            }
            return;
        }
        for index in &related {
            if self.state.pending[*index].id.is_some() {
                continue;
            }
            let id = self.fresh_pending_id();
            let context = self.state.pending[*index].context.clone();
            let code = self.state.pending[*index].code.clone();
            for older in &mut self.state.pending {
                if older.controller == pending.controller
                    && older.reference == pending.reference
                    && older.context == context
                    && older.code == code
                    && older.event.is_null()
                {
                    older.id = Some(id.clone());
                }
            }
        }
        pending.id = related.iter().find_map(|index| {
            let other = &self.state.pending[*index];
            (other.context == pending.context && other.code == pending.code)
                .then(|| other.id.clone())
                .flatten()
        });
        if pending.id.is_none() {
            pending.id = Some(self.fresh_pending_id());
        }
    }

    fn fresh_pending_id(&mut self) -> String {
        loop {
            let id = format!("pending-{}", self.group());
            if self
                .state
                .pending
                .iter()
                .all(|pending| pending.id.as_deref() != Some(&id))
            {
                return id;
            }
        }
    }
}
