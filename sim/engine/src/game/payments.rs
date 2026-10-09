#![expect(
    clippy::indexing_slicing,
    reason = "JSON reads are total and writes use constructed maps."
)]
use super::{Frame, Game, int, list, string};
use crate::{EngineFailure, Result};
use core::slice::from_ref;
use serde_json::{Value, json};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Payment preparation and execution share game state."
)]
impl Game {
    pub(super) fn movement_payment_possible(&self, costs: &[Value], frame: &Frame) -> Result<bool> {
        // Replay costs in an isolated state so earlier departures and replacements affect capacity.
        let mut preview = self.clone();
        preview.state.prompt = None;
        preview.state.frame = None;
        preview.emitted.clear();
        let mut context = frame.clone();
        context.todo = costs.to_vec();
        context.values.insert("paying_cost".into(), json!(true));
        let mut fuel = 20_000_u32;
        while !context.todo.is_empty() {
            fuel = fuel.checked_sub(1).ok_or_else(|| {
                EngineFailure::Unsupported("cost preview exhausted its execution budget".into())
            })?;
            let node = context.todo.remove(0);
            if matches!(string(&node["op"]), "move" | "banish" | "discard")
                && let Some(top) = node["subjects"].get("top")
            {
                let required = preview.number(top, &context)?.max(0);
                let available = i64::try_from(preview.select(&node["subjects"], &context)?.len())
                    .unwrap_or(i64::MAX);
                if available < required {
                    return Ok(false);
                }
            }
            if matches!(string(&node["op"]), "pp" | "counter" | "act" | "flip")
                && !preview.can_pay(from_ref(&node), &context)?
            {
                return Ok(false);
            }
            preview.execute(&node, &mut context)?;
            if let Some(prompt) = &preview.state.prompt {
                if prompt.resume["resume"] == "move-capacity" {
                    return Ok(false);
                }
                return Err(EngineFailure::Unsupported(
                    "cost preview requires an unplanned input".into(),
                ));
            }
        }
        Ok(true)
    }

    fn payment_selection(node: &Value, frame: &Frame) -> Result<Option<Value>> {
        let selections = list(&node["cost_selections"])
            .into_iter()
            .filter(|spec| Self::mode_applies(spec, frame))
            .collect::<Vec<_>>();
        if selections.len() > 1
            || selections
                .iter()
                .any(|spec| spec.get("distribute").is_some())
        {
            return Err(EngineFailure::Unsupported(
                "resolution payment needs one flat material selection without distribution".into(),
            ));
        }
        Ok(selections.into_iter().next())
    }

    fn resolution_payment_context(frame: &Frame) -> Frame {
        let mut context = frame.clone();
        context.values.remove("additional_nodes");
        context.values.remove("additional_pp");
        context.decision["do"] = json!("resolve-choice");
        context
    }

    pub(super) fn resolution_payment(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let selection = Self::payment_selection(node, frame)?;
        let mut context = Self::resolution_payment_context(frame);
        if selection.is_some() {
            context.decision["costs"] = json!({});
        }
        let mut choices = Vec::new();
        for option in self.parameterize(context.decision.clone(), node, &context)? {
            let mut choice = json!({"do":"resolve-choice","choice":"execute"});
            if let Some(spec) = &selection {
                let field = if spec["order"] == true {
                    "order"
                } else {
                    "select"
                };
                choice[field] = option["costs"][string(&spec["key"])].clone();
            }
            choices.push(choice);
        }
        choices.push(json!({"do":"resolve-choice","choice":"decline"}));
        self.prompt(frame, choices, json!({"resume":"pay","node":node}));
        Ok(())
    }

    pub(super) fn resume_payment(
        &mut self,
        node: &Value,
        decision: &Value,
        frame: &mut Frame,
    ) -> Result<()> {
        if decision["choice"] != "execute" {
            if !node["else"].is_null() {
                Self::prepend(frame, vec![node["else"].clone()]);
            }
            return Ok(());
        }
        let mut restore = json!({"op":"_restore_payment_selection","costs":frame.decision.get("costs"),"captured":{}});
        if let Some(spec) = Self::payment_selection(node, frame)? {
            let field = if spec["order"] == true {
                "order"
            } else {
                "select"
            };
            frame.decision["costs"] = json!({string(&spec["key"]):decision[field]});
            for id in list(&decision[field]) {
                let id = string(&id);
                restore["captured"][id] = frame.captured.get(id).cloned().unwrap_or_default();
                frame
                    .captured
                    .insert(id.into(), self.object_attributes(id)?);
            }
        }
        let mut context = Self::resolution_payment_context(frame);
        if !self.valid_parameters(node, &context)?
            || !self.can_pay(&list(&node["costs"]), &context)?
        {
            return Err(crate::invalid("resolution payment is no longer payable"));
        }
        self.pay_costs(node, &mut context)?;
        frame.paid = true;
        Self::prepend(frame, vec![node["then"].clone(), restore]);
        Ok(())
    }

    pub(super) fn restore_payment_selection(node: &Value, frame: &mut Frame) {
        if node["costs"].is_null() {
            frame
                .decision
                .as_object_mut()
                .map(|map| map.remove("costs"));
        } else {
            frame.decision["costs"] = node["costs"].clone();
        }
        if let Some(captured) = node["captured"].as_object() {
            for (id, attributes) in captured {
                if attributes.is_null() {
                    frame.captured.remove(id);
                } else {
                    frame.captured.insert(id.clone(), attributes.clone());
                }
            }
        }
    }

    pub(super) fn additional_choices(
        &self,
        base: Value,
        code: &Value,
        frame: &Frame,
    ) -> Result<Vec<Value>> {
        let specifications = list(&code["additional_costs"]);
        let mut options = vec![base];
        for spec in &specifications {
            let slot = if specifications.len() == 1 {
                "additional"
            } else {
                string(&spec["key"])
            };
            let mut expanded = Vec::new();
            for option in options {
                expanded.extend(self.additional_group_choices(option, spec, slot, frame)?);
            }
            options = expanded;
        }
        Ok(options)
    }

    fn additional_group_choices(
        &self,
        base: Value,
        spec: &Value,
        slot: &str,
        frame: &Frame,
    ) -> Result<Vec<Value>> {
        let mut context = frame.clone();
        context.decision = base.clone();
        if !Self::mode_applies(spec, &context) {
            return Ok(vec![base]);
        }
        let mut decline = base.clone();
        decline["optional_costs"][slot] = json!("decline");
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
                    option["optional_costs"][slot] = json!({"stack_counter_from":id});
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
            external["optional_costs"][slot] = payload;
            result.push(external);
        }
        Ok(result)
    }

    pub(super) fn prepare_additional(&self, code: &Value, frame: &mut Frame) -> Result<bool> {
        let specifications = list(&code["additional_costs"]);
        if specifications.is_empty() {
            return Ok(true);
        }
        let mut prepared = Vec::new();
        let mut extra_pp = 0_i64;
        for spec in &specifications {
            let slot = if specifications.len() == 1 {
                "additional"
            } else {
                string(&spec["key"])
            };
            let key = format!("paid.{}", string(&spec["key"]));
            frame.values.insert(key.clone(), json!(false));
            if !Self::mode_applies(spec, frame) {
                continue;
            }
            let payload = frame.decision["optional_costs"][slot].clone();
            if payload.is_null() || payload == "decline" {
                continue;
            }
            if !payload.is_object() {
                return Ok(false);
            }
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
        }
        frame
            .values
            .insert("additional_nodes".into(), json!(prepared));
        frame.values.insert("additional_pp".into(), json!(extra_pp));
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
            _ => return Err(crate::invalid("unknown payment continuation")),
        }
        Ok(())
    }
}
