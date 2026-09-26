#![expect(
    clippy::indexing_slicing,
    reason = "JSON reads are total and writes use constructed maps."
)]
use super::{Frame, Game, int, list, string};
use crate::{EngineFailure, Result};
use serde_json::{Value, json};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Payment preparation and execution share game state."
)]
impl Game {
    pub(super) fn additional_choices(
        &self,
        base: Value,
        code: &Value,
        frame: &Frame,
    ) -> Result<Vec<Value>> {
        let specifications = list(&code["additional_costs"]);
        if specifications.len() > 1 {
            return Err(EngineFailure::Unsupported(
                "multiple independent additional-cost groups".into(),
            ));
        }
        let Some(spec) = specifications.first() else {
            return Ok(vec![base]);
        };
        let mut context = frame.clone();
        context.decision = base.clone();
        if !Self::mode_applies(spec, &context) {
            return Ok(vec![base]);
        }
        let mut decline = base.clone();
        decline["optional_costs"]["additional"] = json!("decline");
        let mut result = vec![decline];
        let costs = list(&spec["costs"]);
        if costs.iter().any(|cost| cost["op"] == "earth_rite") {
            let count = costs
                .iter()
                .find(|cost| cost["op"] == "earth_rite")
                .map_or(0, |cost| int(&cost["count"]));
            for id in self.select(
                &json!({"side":"self","zone":"field","keyword":"stack"}),
                frame,
            )? {
                if int(&self.object(&id)?.state["counters"]["stack_counter"]) >= count {
                    let mut option = base.clone();
                    option["optional_costs"]["additional"] = json!({"stack_counter_from":id});
                    result.push(option);
                }
            }
            return Ok(result);
        }
        let synthetic = json!({"cost_selections":spec["cost_selections"]});
        for option in self.parameterize(base, &synthetic, frame)? {
            let mut payload = json!({});
            for cost in &costs {
                let op = string(&cost["op"]);
                if op == "pp" {
                    payload["pp"] = json!(self.number(&cost["amount"], &context)?);
                } else if let Some(key) = string(&cost["subjects"]).strip_prefix("cost.") {
                    payload[if op == "move" { "return_to_deck" } else { op }] =
                        option["costs"][key].clone();
                } else {
                    return Err(EngineFailure::Unsupported(
                        "additional cost without explicit selection binding".into(),
                    ));
                }
            }
            let mut external = option;
            external.as_object_mut().map(|map| map.remove("costs"));
            external["optional_costs"]["additional"] = payload;
            result.push(external);
        }
        Ok(result)
    }

    pub(super) fn prepare_additional(&self, code: &Value, frame: &mut Frame) -> Result<bool> {
        let specifications = list(&code["additional_costs"]);
        if specifications.len() > 1 {
            return Err(EngineFailure::Unsupported(
                "multiple independent additional-cost groups".into(),
            ));
        }
        for spec in specifications {
            let key = format!("paid.{}", string(&spec["key"]));
            frame.values.insert(key.clone(), json!(false));
            if !Self::mode_applies(&spec, frame) {
                continue;
            }
            let payload = frame.decision["optional_costs"]["additional"].clone();
            if payload.is_null() || payload == "decline" {
                continue;
            }
            if !payload.is_object() {
                return Ok(false);
            }
            let mut prepared = Vec::new();
            let mut extra_pp = 0_i64;
            for cost in list(&spec["costs"]) {
                let op = string(&cost["op"]);
                if op == "pp" {
                    let amount = self.number(&cost["amount"], frame)?;
                    if payload["pp"] != amount {
                        return Ok(false);
                    }
                    extra_pp = extra_pp.saturating_add(amount);
                    prepared.push(cost);
                } else if op == "earth_rite" {
                    let Some(id) = payload["stack_counter_from"].as_str() else {
                        return Ok(false);
                    };
                    if !self
                        .select(
                            &json!({"side":"self","zone":"field","keyword":"stack"}),
                            frame,
                        )?
                        .contains(&id.into())
                        || int(&self.object(id)?.state["counters"]["stack_counter"])
                            < self.number(&cost["count"], frame)?
                    {
                        return Ok(false);
                    }
                    prepared.push(json!({"op":"_earth_payment","object":id,"count":cost["count"]}));
                } else if let Some(selection) = string(&cost["subjects"]).strip_prefix("cost.") {
                    let values = payload[if op == "move" { "return_to_deck" } else { op }].clone();
                    frame.decision["costs"][selection] = values.clone();
                    for id in list(&values) {
                        let id = string(&id);
                        if self.state.objects.contains_key(id) {
                            frame
                                .captured
                                .insert(id.into(), self.object_attributes(id)?);
                        }
                    }
                    prepared.push(cost);
                } else {
                    return Err(EngineFailure::Unsupported(
                        "additional cost without explicit selection binding".into(),
                    ));
                }
            }
            if !self.valid_parameters(&json!({"cost_selections":spec["cost_selections"]}), frame)? {
                return Ok(false);
            }
            frame.values.insert(key, json!(true));
            frame
                .values
                .insert("additional_nodes".into(), json!(prepared));
            frame.values.insert("additional_pp".into(), json!(extra_pp));
        }
        Ok(true)
    }

    pub(super) fn payment_nodes(code: &Value, frame: &Frame) -> Vec<Value> {
        let mut costs = list(&code["costs"]);
        costs.extend(
            list(frame.values.get("additional_nodes").unwrap_or(&Value::Null))
                .into_iter()
                .filter(|cost| frame.decision["do"] != "play" || cost["op"] != "pp"),
        );
        costs
    }
    pub(super) fn payment_effect(&mut self, node: &Value, frame: &Frame) -> Result<()> {
        match string(&node["op"]) {
            "_earth_payment" => {
                let id = string(&node["object"]);
                let count = self.number(&node["count"], frame)?;
                let previous = int(&self.object(id)?.state["counters"]["stack_counter"]);
                self.object_mut(id)?.state["counters"]["stack_counter"] =
                    json!(previous.saturating_sub(count));
                let group = self.group();
                self.emit(json!({"kind":"カウンター","object":id,"name":self.catalog.keyword_name("stack_counter"),"delta":count.saturating_neg()}),&frame.cause,group);
                if previous == count {
                    self.move_objects(&[id.into()], "cemetery", None, None, frame)?;
                }
            }
            "extra_turn" => {
                for seat in self.seats(string(&node["side"]), frame) {
                    let mut turns = list(&self.state.turn["extra_turns"]);
                    for _ in 0..self.number(&node["count"], frame)?.max(0) {
                        turns.push(json!(seat));
                    }
                    self.state.turn["extra_turns"] = json!(turns);
                }
            }
            _ => return Err(crate::invalid("unknown payment continuation")),
        }
        Ok(())
    }
}
