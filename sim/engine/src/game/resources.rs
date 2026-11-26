#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON has total read indexing and constructed write maps."
)]
use super::{EventOccurrence, Frame, Game, int, list};
use crate::{Result, invalid};
use serde_json::{Value, json};

impl Game {
    pub(super) fn import_event_occurrences(&mut self, semantic: &Value) -> Result<()> {
        for value in list(&semantic["event_occurrences"]) {
            let entry: EventOccurrence = serde_json::from_value(value).map_err(invalid)?;
            self.object(&entry.subject)?;
            let key = json!([entry.event, entry.subject]).to_string();
            if entry.count == 0 || self.state.event_occurrences.insert(key, entry).is_some() {
                return Err(invalid("event occurrences must be unique positive records"));
            }
        }
        Ok(())
    }

    pub(super) fn stack_replaces_departure(&self, id: &str, zone: &str) -> Result<bool> {
        let object = self.object(id)?;
        Ok(object.zone == "field"
            && zone != "field"
            && self.keywords(id)?.contains("stack")
            && int(&object.state["counters"]["stack_counter"]) > 0)
    }

    pub(super) fn initialize_entry_counters(&mut self, id: &str, frame: &Frame) -> Result<()> {
        if self.keywords(id)?.contains("stack") {
            let count = frame
                .values
                .get("stack_entry_counts")
                .and_then(|counts| counts.get(id))
                .map_or(1, int);
            let old = int(&self.object(id)?.state["counters"]["stack_counter"]);
            self.object_mut(id)?.state["counters"]["stack_counter"] =
                json!(old.saturating_add(count));
        }
        Ok(())
    }
}
