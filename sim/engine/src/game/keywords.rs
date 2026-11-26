#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::BTreeSet;
use serde_json::Value;

use super::{Game, list};
use crate::Result;

impl Game {
    pub(super) fn keywords(&self, id: &str) -> Result<BTreeSet<String>> {
        let mut keywords = match self.internal_keywords(id)? {
            Some(keywords) => keywords,
            None => self.setup_keywords(id)?,
        };
        keywords.extend(self.internal_granted_keywords(id)?);
        Ok(keywords)
    }

    pub(super) fn setup_keywords(&self, id: &str) -> Result<BTreeSet<String>> {
        let object = self.object(id)?;
        let mut result: BTreeSet<String> = list(&object.state["keywords"])
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        if !list(&object.state["links"]["出走"]).is_empty() {
            result.insert("rush".into());
        }
        Ok(result)
    }
}
