#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use core::mem::take;
use serde_json::{Value, json};

use super::{Frame, Game, int, list, scalar, string};
use crate::{EngineFailure, Result, invalid};

fn cost_nodes(node: &Value, result: &mut Vec<Value>) {
    if node["op"] == "adjust_cost" {
        result.push(node.clone());
        return;
    }
    match node {
        Value::Object(fields) => {
            for value in fields.values() {
                cost_nodes(value, result);
            }
        }
        Value::Array(items) => {
            for value in items {
                cost_nodes(value, result);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Cost layers and their consumption share the authoritative game state."
)]
impl Game {
    pub(super) fn import_cost_history(&mut self) -> Result<()> {
        let mut entries = Vec::new();
        for mut entry in take(&mut self.state.continuous) {
            if !entry["effect"].is_null() || !string(&entry["text"]).contains("コスト") {
                entries.push(entry);
                continue;
            }
            let source = string(&entry["source"]);
            let node = self.historical_cost_node(source, string(&entry["text"]))?;
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

    fn historical_cost_node(&self, source: &str, text: &str) -> Result<Value> {
        let object = self.object(source)?;
        let number = object.state["evolved_with"]
            .as_str()
            .and_then(|id| self.state.objects.get(id))
            .map_or(object.card.as_str(), |evolved| evolved.card.as_str());
        let face = self.face(source)?;
        let mut candidates = Vec::new();
        for code in list(&self.catalog.program(number)?["abilities"]) {
            if code
                .get("face")
                .is_some_and(|index| int(index) != int(&object.state["face"]))
            {
                continue;
            }
            let printed = code["section"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| face["sections"].get(index))
                .unwrap_or_else(|| &face["text"]);
            let line = usize::try_from(int(&code["line"]).saturating_sub(1)).map_err(invalid)?;
            if string(printed)
                .lines()
                .nth(line)
                .is_some_and(|line| line.contains(text.trim().trim_end_matches('。')))
            {
                cost_nodes(&code["body"], &mut candidates);
            }
        }
        if candidates.len() != 1 {
            return Err(EngineFailure::Unsupported(
                "historical cost text does not identify one authored effect".into(),
            ));
        }
        Ok(candidates.remove(0))
    }

    fn cost_effect_applies(&self, entry: &Value, id: &str) -> Result<bool> {
        let effect = &entry["effect"];
        if effect["op"] != "adjust_cost"
            || effect["kind"] == "evolve"
            || effect.get("uses").is_some_and(|uses| int(uses) <= 0)
        {
            return Ok(false);
        }
        let frame: Frame = serde_json::from_value(entry["context"].clone()).map_err(invalid)?;
        if let Some(condition) = effect.get("condition")
            && !self.truth(condition, &frame)?
        {
            return Ok(false);
        }
        if entry["dynamic"] == true {
            return Ok(
                list(&entry["applies_to"]).contains(&json!(self.object(id)?.controller))
                    && self.matches(id, &effect["subjects"], &frame)?,
            );
        }
        Ok(self.continuous_applies(entry, id))
    }

    pub(super) fn consume_cost_adjustments(&mut self, id: &str) -> Result<()> {
        let mut consumed = Vec::new();
        for (index, entry) in self.state.continuous.iter().enumerate() {
            if entry["effect"].get("uses").is_some() && self.cost_effect_applies(entry, id)? {
                consumed.push(index);
            }
        }
        for index in consumed.into_iter().rev() {
            let remaining = int(&self.state.continuous[index]["effect"]["uses"]).saturating_sub(1);
            if remaining == 0 {
                self.state.continuous.remove(index);
            } else {
                self.state.continuous[index]["effect"]["uses"] = json!(remaining);
            }
        }
        Ok(())
    }

    pub(super) fn play_cost_context(&self, id: &str, context: &Frame) -> Result<i64> {
        let mut base = scalar(&self.face(id)?["cost"]);
        let mut adjustment = 0_i64;
        let mut sources = self.field_ids();
        if !sources.iter().any(|source| source == id) {
            sources.push(id.into());
        }
        for source in sources {
            for code in self.abilities(&source)? {
                let body = &code["body"];
                if code["kind"] != "static"
                    || body["op"] != "adjust_cost"
                    || body["kind"] == "evolve"
                {
                    continue;
                }
                let mut frame = self.frame_for(&source)?;
                frame.values = context.values.clone();
                frame.decision = context.decision.clone();
                if !self.matches(id, &body["subjects"], &frame)? {
                    continue;
                }
                if let Some(condition) = body.get("condition")
                    && !self.truth(condition, &frame)?
                {
                    continue;
                }
                if body.get("uses").is_some() || body.get("until").is_some() {
                    return Err(EngineFailure::Unsupported(
                        "limited-use static cost adjustment".into(),
                    ));
                }
                if let Some(set) = body.get("set") {
                    base = self.number(set, &frame)?;
                }
                adjustment = adjustment.saturating_add(self.number(&body["amount"], &frame)?);
            }
        }
        for entry in &self.state.continuous {
            if !self.cost_effect_applies(entry, id)? {
                continue;
            }
            let frame: Frame = serde_json::from_value(entry["context"].clone()).map_err(invalid)?;
            if let Some(value) = entry["effect"].get("set") {
                base = self.number(value, &frame)?;
            }
            adjustment =
                adjustment.saturating_add(self.number(&entry["effect"]["amount"], &frame)?);
        }
        Ok(base
            .saturating_add(adjustment)
            .saturating_add(int(context
                .values
                .get("additional_pp")
                .unwrap_or(&Value::Null)))
            .max(0))
    }
}
