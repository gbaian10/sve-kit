#![expect(
    clippy::indexing_slicing,
    reason = "Historical declarations and authored nodes use total JSON indexing."
)]
use alloc::collections::{BTreeMap, BTreeSet};
use core::mem::take;
use serde_json::{Value, json};

use super::{Game, int, list, string};
use crate::{EngineFailure, Result, invalid};

const NUMERIC: [&str; 4] = ["power", "hp", "set_power", "set_hp"];

fn normalized_text(text: &str) -> String {
    text.trim()
        .trim_end_matches('。')
        .replace(['{', '}'], "")
        .replace("これは", "それは")
        .replace("これの", "それの")
}

fn effect_nodes(node: &Value, operation: &str, result: &mut Vec<Value>) {
    if node["op"] == operation
        && (operation != "modify" || NUMERIC.iter().any(|key| node.get(*key).is_some()))
    {
        result.push(node.clone());
        return;
    }
    match node {
        Value::Object(fields) => {
            for value in fields.values() {
                effect_nodes(value, operation, result);
            }
        }
        Value::Array(items) => {
            for value in items {
                effect_nodes(value, operation, result);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn check_numeric_history(entry: &Value, node: &Value) -> Result<i64> {
    if !matches!(string(&entry["until"]), "game" | "permanent")
        || node.as_object().is_none_or(|fields| {
            fields.keys().any(|key| {
                !matches!(key.as_str(), "op" | "subjects") && !NUMERIC.contains(&key.as_str())
            })
        })
        || NUMERIC
            .iter()
            .any(|key| node.get(*key).is_some_and(|value| !value.is_i64()))
    {
        return Err(EngineFailure::Unsupported(
            "historical numeric effect needs a literal permanent modifier".into(),
        ));
    }
    entry["order"].as_i64().ok_or_else(|| {
        EngineFailure::Unsupported("historical numeric effect needs an ordered timestamp".into())
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "History imports resolve existing authored nodes against the authoritative catalog."
)]
impl Game {
    pub(super) fn historical_effect(
        &self,
        source: &str,
        text: &str,
        operation: &str,
    ) -> Result<Value> {
        let object = self.information_source(source)?;
        let number = object.card.as_str();
        let face = self.face(source)?;
        let text = normalized_text(text);
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
            if !text.is_empty()
                && string(printed)
                    .lines()
                    .nth(line)
                    .is_some_and(|line| normalized_text(line).contains(&text))
            {
                effect_nodes(&code["body"], operation, &mut candidates);
            }
        }
        if candidates.len() != 1 {
            return Err(EngineFailure::Unsupported(
                "historical text does not identify one authored effect".into(),
            ));
        }
        Ok(candidates.remove(0))
    }

    pub(super) fn import_numeric_history(&mut self, setup: &Value) -> Result<()> {
        let mut entries = take(&mut self.state.continuous);
        let mut ordered = BTreeMap::new();
        for (index, entry) in entries.iter_mut().enumerate() {
            if !entry["effect"].is_null() {
                continue;
            }
            let node =
                self.historical_effect(string(&entry["source"]), string(&entry["text"]), "modify")?;
            let order = check_numeric_history(entry, &node)?;
            if ordered.insert(order, index).is_some() {
                return Err(EngineFailure::Unsupported(
                    "numeric history has unresolved simultaneous timestamps".into(),
                ));
            }
            entry["effect"] = node;
            entry["generations"] = json!({});
            for id in list(&entry["applies_to"]) {
                entry["generations"][string(&id)] = json!(self.object(string(&id))?.generation);
            }
        }
        let mut affected = BTreeSet::new();
        for index in ordered.values() {
            let entry = &entries[*index];
            let context = self.frame_for(string(&entry["source"]))?;
            for id in list(&entry["applies_to"]) {
                let id = string(&id);
                self.modify_numbers(id, &entry["effect"], &context)?;
                affected.insert(id.to_owned());
            }
        }
        self.state.continuous = entries;
        for id in affected {
            self.restore_numeric_overrides(&id, setup)?;
        }
        Ok(())
    }

    fn restore_numeric_overrides(&mut self, id: &str, setup: &Value) -> Result<()> {
        let object = self.object(id)?;
        let patch = list(&setup["players"][&object.controller]["zones"][&object.zone])
            .into_iter()
            .find(|item| item["id"] == id)
            .unwrap_or(Value::Null);
        let state = &mut self.object_mut(id)?.state;
        for field in ["power", "hp", "max_hp"] {
            if let Some(value) = patch["state"].get(field) {
                state[field] = value.clone();
            }
        }
        if patch["state"].get("hp").is_none()
            && let Some(damage) = patch["state"].get("damage")
        {
            state["hp"] = json!(int(&state["max_hp"]).saturating_sub(int(damage)));
        }
        Ok(())
    }
}
