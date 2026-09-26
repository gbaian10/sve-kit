#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use super::{Frame, Game, Object, Pending, int, list, string};
use crate::{EngineFailure, Result, invalid};
use core::mem::take;
use serde_json::{Value, json};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn change_life(&mut self, seat: &str, life: i64) -> Result<()> {
        let previous = int(&self.player(seat)?.leader["life"]);
        self.player_mut(seat)?.leader["life"] = json!(life);
        if life < previous {
            self.bump(&format!("{seat}.leader_hp_decreased"), 1);
        }
        if life > previous {
            self.bump(
                &format!("{seat}.leader_hp_increased"),
                life.saturating_sub(previous),
            );
        }
        Ok(())
    }

    pub(super) fn check_execution_parameters(node: &Value) -> Result<()> {
        let denied: &[&str] = match string(&node["op"]) {
            "modify" => &["type", "abilities", "during", "traits", "cost", "set_cost"],
            "select" | "search" => &["distinct_by"],
            "pay" => &["cost_selections"],
            "draw" | "look" => &["up_to"],
            _ => &[],
        };
        for field in denied {
            if node.get(*field).is_some() {
                return Err(EngineFailure::Unsupported(format!(
                    "{}.{field} is declarative-only",
                    string(&node["op"])
                )));
            }
        }
        Ok(())
    }

    pub(super) fn register_delay(&mut self, node: &Value, frame: &Frame) -> Result<()> {
        if !matches!(
            string(&node["event"]),
            "end" | "field_to_cemetery" | "leave"
        ) {
            return Err(EngineFailure::Unsupported(
                "delayed event not implemented".into(),
            ));
        }
        let mut subjects = Vec::new();
        if let Some(selector) = node.get("subjects") {
            for id in self.select(selector, frame)? {
                subjects.push(json!({"id":id,"generation":self.object(&id)?.generation}));
            }
        }
        let mut context = frame.clone();
        context.todo.clear();
        context.frozen.clear();
        context.cause = Value::Null;
        self.state.delayed.push(json!({"source":frame.source,"controller":frame.controller,"reference":frame.reference,"event":node["event"],"body":node["body"],"once":node["once"],"context":context,"subjects":subjects,"until":node["until"]}));
        Ok(())
    }

    pub(super) fn collect_delayed(
        &mut self,
        event: &str,
        affected: &[Object],
        cause: &Value,
    ) -> Result<Vec<Pending>> {
        let mut batch = Vec::new();
        for entry in take(&mut self.state.delayed) {
            let matches = entry["event"] == event
                && affected.iter().any(|object| {
                    list(&entry["subjects"]).iter().any(|subject| {
                        subject["id"] == object.id
                            && subject["generation"].as_u64() == Some(object.generation)
                    })
                });
            if !matches {
                self.state.delayed.push(entry);
                continue;
            }
            let context: Frame =
                serde_json::from_value(entry["context"].clone()).map_err(invalid)?;
            let mut reference = entry["reference"].clone();
            reference["delayed"] = json!(true);
            batch.push(Pending{controller:context.controller.clone(),reference,event:Value::Null,code:json!({"kind":"trigger","line":entry["reference"]["line"],"body":entry["body"]}),source:context.source.clone(),cause:cause.clone(),retained:false,id:None,context:Some(context)});
            if entry["once"] != true {
                self.state.delayed.push(entry);
            }
        }
        Ok(batch)
    }

    pub(super) fn enqueue_delayed_end(&mut self) -> Result<()> {
        let mut batch = Vec::new();
        for entry in take(&mut self.state.delayed) {
            if entry["event"] != "end" || entry["controller"] != self.active() {
                self.state.delayed.push(entry);
                continue;
            }
            let source = string(&entry["source"]).to_owned();
            let reference = if entry["reference"].is_null() {
                json!({"source":source,"line":0_i64,"rule":"10.8"})
            } else {
                entry["reference"].clone()
            };
            let context: Frame =
                serde_json::from_value(entry["context"].clone()).map_err(invalid)?;
            batch.push(Pending {
                controller: context.controller.clone(),
                reference,
                event: json!({"delayed_end":true}),
                code: json!({"kind":"trigger","line":0_i64,"event":"end","body":entry["body"]}),
                source,
                cause: json!({"decision":self.node}),
                retained: false,
                id: None,
                context: Some(context),
            });
            if entry["once"] != true {
                self.state.delayed.push(entry);
            }
        }
        self.enqueue(batch);
        Ok(())
    }

    pub(super) fn expire_silence(&mut self) -> Result<()> {
        self.state
            .delayed
            .retain(|entry| entry["until"] != "end-of-turn");
        let mut active = Vec::new();
        let mut expired = Vec::new();
        for entry in take(&mut self.state.continuous) {
            if entry["until"] == "end-of-turn" && entry["effect"]["remove_abilities"] == true {
                expired.push(entry);
                continue;
            }
            if entry["until"] != "end-of-turn" {
                active.push(entry);
            }
        }
        for entry in expired.into_iter().rev() {
            for id in list(&entry["applies_to"]) {
                let id = string(&id);
                if Some(self.object(id)?.generation) != entry["generation"].as_u64() {
                    continue;
                }
                if active.iter().any(|other| {
                    other["effect"]["remove_abilities"] == true
                        && list(&other["applies_to"]).contains(&json!(id))
                        && other["order"].as_u64() > entry["order"].as_u64()
                }) {
                    continue;
                }
                let mut keywords = list(&entry["prior_keywords"]);
                keywords.extend(list(&self.object(id)?.state["keywords"]));
                self.object_mut(id)?.state["silenced"] = entry["prior_silenced"].clone();
                self.object_mut(id)?.state["keywords"] = json!(keywords);
            }
        }
        self.state.continuous = active;
        Ok(())
    }
}
