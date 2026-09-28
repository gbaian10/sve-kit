#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use serde_json::Value;

use super::{Game, Pending, TriggerAlternative};
use crate::{EngineFailure, Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Trigger formation shares the authoritative usage and pending state."
)]
impl Game {
    pub(super) fn limit_trigger_batch(
        &mut self,
        source: &str,
        code: &Value,
        mut batch: Vec<Pending>,
    ) -> Result<Vec<Pending>> {
        if code["limit_at"] != "trigger" || batch.is_empty() {
            return Ok(batch);
        }
        let Some(remaining) = self.remaining_uses(source, code)? else {
            return Ok(batch);
        };
        if remaining == 0 {
            return Ok(Vec::new());
        }
        let count = u64::try_from(batch.len()).map_err(invalid)?;
        if count > remaining {
            let mut alternatives = Vec::new();
            for pending in &batch {
                let alternative = TriggerAlternative {
                    event: pending.event.clone(),
                    context: pending
                        .context
                        .clone()
                        .ok_or_else(|| invalid("missing trigger context"))?,
                };
                if !alternatives.contains(&alternative) {
                    alternatives.push(alternative);
                }
            }
            if alternatives.len() == 1 {
                batch.truncate(usize::try_from(remaining).map_err(invalid)?);
            } else if remaining == 1 {
                if alternatives.iter().enumerate().any(|(index, alternative)| {
                    alternative.event.is_null()
                        || alternatives
                            .iter()
                            .skip(index.saturating_add(1))
                            .any(|other| other.event == alternative.event)
                }) {
                    return Err(EngineFailure::Unsupported(
                        "limited trigger alternatives need distinct public event references".into(),
                    ));
                }
                batch.truncate(1);
                let pending = batch
                    .first_mut()
                    .ok_or_else(|| invalid("missing limited trigger"))?;
                pending.event = Value::Null;
                pending.context = None;
                pending.alternatives = alternatives;
            } else {
                return Err(EngineFailure::Unsupported(
                    "selecting multiple limited trigger conditions needs a joint formation choice"
                        .into(),
                ));
            }
        }
        for _ in &batch {
            self.mark_use(source, code)?;
        }
        Ok(batch)
    }

    pub(super) fn pending_variants(pending: &Pending) -> Vec<Pending> {
        if pending.alternatives.is_empty() {
            return vec![pending.clone()];
        }
        let mut base = pending.clone();
        base.alternatives.clear();
        pending
            .alternatives
            .iter()
            .map(|alternative| {
                let mut variant = base.clone();
                variant.event.clone_from(&alternative.event);
                variant.context = Some(alternative.context.clone());
                variant.retained = true;
                variant
            })
            .collect()
    }

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
