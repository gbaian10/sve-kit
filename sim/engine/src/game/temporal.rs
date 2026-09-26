#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use super::{Frame, Game, Object, Pending, int, list, other, string};
use crate::{EngineFailure, Result, invalid};
use core::mem::take;
use serde_json::{Value, json};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn life_change_triggers(
        &mut self,
        changes: &[(String, i64, i64)],
        cause: &Value,
    ) -> Result<()> {
        let mut pending = Vec::new();
        for (seat, before, after) in changes {
            if before == after {
                continue;
            }
            let subject = self.event_subject(&format!("{seat}.leader"))?;
            pending.extend(self.collect_subject_event(
                "leader_life_change",
                &[subject],
                cause,
                &json!({"before_life":before,"after_life":after}),
            )?);
        }
        self.enqueue(pending);
        Ok(())
    }

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
            let mut reference = if entry["reference"].is_null() {
                json!({"source":source,"line":0_i64,"rule":"10.8"})
            } else {
                entry["reference"].clone()
            };
            reference["delayed"] = json!(true);
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

    pub(super) fn capture_restriction_period(
        &self,
        entry: &mut Value,
        id: &str,
        frame: &Frame,
    ) -> Result<()> {
        if entry["effect"]["op"] != "restrict" || entry["during"].is_null() {
            return Ok(());
        }
        let during = string(&entry["during"]);
        let (seat, phase) = if let Some(phase) = during.strip_prefix("next-opponent-") {
            (other(&frame.controller), phase)
        } else if let Some(phase) = during.strip_prefix("next-controller-") {
            let seat = if let Some(seat) = id.strip_suffix(".leader") {
                seat
            } else {
                &self.object(id)?.controller
            };
            (seat, phase)
        } else {
            return Err(EngineFailure::Unsupported(format!(
                "restriction period: {during}"
            )));
        };
        if !matches!(phase, "start" | "main" | "turn") {
            return Err(EngineFailure::Unsupported(format!(
                "restriction phase: {phase}"
            )));
        }
        let phase = phase.to_owned();
        entry["window_player"] = json!(seat);
        entry["window_turn"] =
            json!(int(&self.state.turn["elapsed_turns"][seat]).saturating_add(1));
        entry["window_phase"] = json!(phase);
        Ok(())
    }

    pub(super) fn expire_effects(&mut self) -> Result<()> {
        self.state
            .delayed
            .retain(|entry| entry["until"] != "end-of-turn");
        let mut active = Vec::new();
        let mut expired = Vec::new();
        for entry in take(&mut self.state.continuous) {
            let should_expire = entry["until"] == "end-of-turn"
                || (entry["window_player"] == self.active()
                    && int(&self.state.turn["elapsed_turns"][self.active()])
                        >= int(&entry["window_turn"]))
                || (entry["until"] == "next-controller-end"
                    && entry["duration_controller"] == self.active()
                    && int(&self.state.turn["elapsed_turns"][self.active()])
                        >= int(&entry["expires_turn"]));
            if should_expire {
                expired.push(entry);
                continue;
            }
            active.push(entry);
        }
        for entry in expired.into_iter().rev() {
            if entry["effect"]["op"] != "pilot" && entry["effect"]["remove_abilities"] != true {
                continue;
            }
            for id in list(&entry["applies_to"]) {
                let id = string(&id);
                if Some(self.object(id)?.generation) != entry["generation"].as_u64() {
                    continue;
                }
                if entry["effect"]["op"] == "pilot" {
                    for field in ["card_type", "power", "hp", "max_hp"] {
                        if let Some(prior) = entry["prior"].get(field) {
                            self.object_mut(id)?.state[field] = prior.clone();
                        } else {
                            self.object_mut(id)?
                                .state
                                .as_object_mut()
                                .ok_or_else(|| invalid("object attributes must be a map"))?
                                .remove(field);
                        }
                    }
                }
                if entry["effect"]["remove_abilities"] != true {
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
