#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::BTreeSet;
use core::slice::from_ref;
use serde_json::{Value, json};

use super::legal::subsets;
use super::{Frame, Game, Object, int, list, scalar, string};
use crate::{EngineFailure, Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn enter_triggers(
        &mut self,
        object: &Object,
        cause: &Value,
        frame: &Frame,
    ) -> Result<()> {
        let reason = if frame.values.get("suppress_fanfare") == Some(&json!(true)) {
            Some(self.suppression_sentence(&frame.reference, "ファンファーレ")?)
        } else {
            None
        };
        let subject = self.event_subject(&object.id)?;
        let entering = self.collect_permitted_event(
            "enter",
            from_ref(&subject),
            cause,
            &Value::Null,
            reason.as_deref(),
        )?;
        self.enqueue(entering);
        Ok(())
    }

    pub(super) fn resolution_select(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let key = if node["order"] == true {
            "order"
        } else {
            "select"
        };
        let choices = self
            .resolution_subsets(node, frame)?
            .into_iter()
            .map(|selected| json!({"do":"resolve-choice",key:selected}))
            .collect();
        self.prompt(
            frame,
            choices,
            json!({"resume":"select","bind":node["bind"],"order":node["order"]}),
        );
        self.set_prompt_side(node, frame);
        Ok(())
    }

    fn set_prompt_side(&mut self, node: &Value, frame: &Frame) {
        let by = self.seats(string(&node["by"]), frame).into_iter().next();
        if let Some(prompt) = self.state.prompt.as_mut()
            && let Some(by) = by
        {
            prompt.by = by;
        }
    }

    pub(super) fn resolution_choice(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let ids = (1..=list(&node["modes"]).len())
            .map(|n| n.to_string())
            .collect::<Vec<_>>();
        let min = usize::try_from(self.number(&node["min"], frame)?.max(0)).map_err(invalid)?;
        let max = usize::try_from(self.number(&node["max"], frame)?.max(0)).map_err(invalid)?;
        let mut labels = list(&node["labels"]);
        for label in &mut labels {
            if let Some(keyword) = label["keyword"].as_str() {
                label["keyword"] = json!(self.catalog.keyword_name(keyword));
            }
        }
        let choices = if labels.is_empty() {
            subsets(&ids,min,max).iter().map(|subset|json!({"do":"resolve-choice","options":subset.iter().filter_map(|n|n.parse::<i64>().ok()).collect::<Vec<_>>()})).collect()
        } else {
            labels
                .iter()
                .cloned()
                .map(|mut label| {
                    label["do"] = json!("resolve-choice");
                    label
                })
                .collect()
        };
        self.prompt(
            frame,
            choices,
            json!({"resume":"choice","modes":node["modes"],"labels":labels}),
        );
        self.set_prompt_side(node, frame);
        Ok(())
    }

    pub(super) fn resume_choice(task: &Value, decision: &Value, frame: &mut Frame) -> Result<()> {
        let labels = list(&task["labels"]);
        let indices = if labels.is_empty() {
            let mut selected = list(&decision["options"]);
            selected.sort_by_key(int);
            selected
                .iter()
                .map(|value| usize::try_from(int(value).saturating_sub(1)).map_err(invalid))
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![
                labels
                    .iter()
                    .position(|label| {
                        label.as_object().is_some_and(|members| {
                            members.iter().all(|(key, value)| decision[key] == *value)
                        })
                    })
                    .ok_or_else(|| invalid("unknown choice label"))?,
            ]
        };
        let modes = list(&task["modes"]);
        let steps = indices
            .into_iter()
            .map(|index| {
                modes
                    .get(index)
                    .cloned()
                    .ok_or_else(|| invalid("choice mode outside range"))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::prepend(frame, steps);
        Ok(())
    }

    pub(super) fn extended_effect(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        match string(&node["op"]) {
            "box" => self.apply_box(node, frame)?,
            "equip" => self.equip_tokens(node, frame)?,
            "pilot" | "become_type" => self.change_type(node, frame)?,
            "flip" => {
                for id in self.select(&node["subjects"], frame)? {
                    self.object_mut(&id)?.state["face_up"] = json!(node["face"] == "up");
                    let group = self.group();
                    self.emit(
                        json!({"kind":"表向き／裏向き","object":id,"face_up":node["face"] == "up"}),
                        &frame.cause,
                        group,
                    );
                }
            }
            "control" => self.change_controller(node, frame)?,
            "create" => self.create_tokens(node, frame)?,
            "counter" => {
                let amount = self.number(&node["amount"], frame)?;
                let name = string(&node["name"]);
                let group = self.group();
                for id in self.select(&node["subjects"], frame)? {
                    let count = int(&self.object(&id)?.state["counters"][name]);
                    self.object_mut(&id)?.state["counters"][name] =
                        json!(count.saturating_add(amount).max(0));
                    self.emit(json!({"kind":"カウンター","object":id,"name":self.catalog.keyword_name(name),"delta":amount}),&frame.cause,group);
                }
            }
            "adjust_cost" | "restrict" | "replace_damage" => {
                self.register_continuous(node, frame)?;
            }
            "reveal_until" => self.reveal_until(node, frame)?,
            unknown => return Err(invalid(format!("unknown extension: {unknown}"))),
        }
        Ok(())
    }

    fn apply_box(&mut self, node: &Value, frame: &Frame) -> Result<()> {
        self.modify(&json!({"op":"modify","subjects":node["subjects"],"remove_abilities":true,"until":"next-controller-end"}),frame)?;
        for id in self.select(&node["subjects"], frame)? {
            self.register_continuous(&json!({"op":"restrict","subjects":id,"action":"normal_stand","until":"next-controller-end"}),frame)?;
        }
        Ok(())
    }

    fn equip_tokens(&mut self, node: &Value, frame: &Frame) -> Result<()> {
        let count = self.number(&node["count"], frame)?.max(0);
        if count > 1000 {
            return Err(EngineFailure::Unsupported(
                "equipment batch exceeds prototype limit".into(),
            ));
        }
        for subject in self.select(&node["subjects"], frame)? {
            if self.object(&subject)?.zone != "field" {
                continue;
            }
            let controller = self.object(&subject)?.controller.clone();
            for _ in 0..count {
                let id = self.new_named_object(string(&node["name"]), &controller)?;
                self.move_objects(from_ref(&id), "equipment", None, None, frame)?;
                self.object_mut(&id)?.state["equipped_to"] = json!(subject);
            }
        }
        Ok(())
    }

    fn change_type(&mut self, node: &Value, frame: &Frame) -> Result<()> {
        for id in self.select(&node["subjects"], frame)? {
            let prior = self.object(&id)?.state.clone();
            let typ = if node["op"] == "pilot" || node["type"] == "follower" {
                "フォロワー"
            } else {
                "アミュレット"
            };
            self.object_mut(&id)?.state["card_type"] = json!(typ);
            if node["op"] == "pilot" {
                for field in ["power", "hp"] {
                    let printed = scalar(&self.face(&id)?[field]);
                    self.object_mut(&id)?.state[field] = json!(printed);
                    if field == "hp" {
                        self.object_mut(&id)?.state["max_hp"] = json!(printed);
                    }
                }
                self.state.continuous.push(json!({"source":frame.source,"applies_to":[id],"generation":self.object(&id)?.generation,"effect":node,"until":"end-of-turn","prior":prior}));
            }
        }
        Ok(())
    }

    fn change_controller(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let controller = self
            .seats(string(&node["side"]), frame)
            .into_iter()
            .next()
            .unwrap_or_default();
        let ids =
            self.select(&node["subjects"], frame)?
                .into_iter()
                .filter(|id| {
                    self.state.objects.get(id).is_some_and(|object| {
                        object.zone == "field" && object.controller != controller
                    })
                })
                .collect::<Vec<_>>();
        let available = usize::try_from(
            5_i64
                .saturating_sub(self.zone_count(&controller, "field"))
                .max(0),
        )
        .map_err(invalid)?;
        if ids.len() > available {
            let choices = subsets(&ids, available, available)
                .iter()
                .map(|chosen| json!({"do":"resolve-choice","select":chosen}))
                .collect();
            self.prompt(
                frame,
                choices,
                json!({"resume":"move-capacity","node":node}),
            );
            return Ok(());
        }
        frame.performed = i64::try_from(ids.len()).unwrap_or(i64::MAX);
        let group = self.group();
        for id in ids {
            let previous = self.object(&id)?.clone();
            self.player_mut(&previous.controller)?
                .zones
                .entry("field".into())
                .or_default()
                .retain(|value| *value != id);
            self.player_mut(&controller)?
                .zones
                .entry("field".into())
                .or_default()
                .push(json!(id));
            let object = self.object_mut(&id)?;
            object.controller.clone_from(&controller);
            object.state["entered_this_turn"] = json!(true);
            self.emit(
                json!({"kind":"移動","object":id,"from":format!("{}.field",previous.controller),"to":format!("{controller}.field")}),
                &frame.cause,
                group,
            );
        }
        Ok(())
    }

    fn register_continuous(&mut self, node: &Value, frame: &Frame) -> Result<()> {
        if node["op"] == "adjust_cost" && node["subjects"]["zone"] == "any" {
            let players = self.seats(string(&node["subjects"]["side"]), frame);
            self.state.continuous.push(json!({"source":frame.source,"controller":frame.controller,"applies_to":players,"dynamic":true,"effect":node,"until":node.get("until").cloned().unwrap_or_else(||json!("game")),"order":self.state.next_event,"context":frame}));
            return Ok(());
        }
        let subjects = self.select(&node["subjects"], frame)?;
        for id in subjects {
            let generation = self.state.objects.get(&id).map(|object| object.generation);
            let mut entry = json!({"source":frame.source,"controller":frame.controller,"applies_to":[id],"generation":generation,"effect":node,"until":node.get("until").cloned().unwrap_or_else(||json!("game")),"during":node["during"],"duration_controller":self.state.objects.get(&id).map(|object|object.controller.clone()),"expires_turn":self.state.objects.get(&id).map_or(0,|object|int(&self.state.turn["elapsed_turns"][&object.controller]).saturating_add(i64::from(self.active()!=object.controller))),"order":self.state.next_event,"context":frame});
            self.capture_effect_period(&mut entry, &id, frame)?;
            self.state.continuous.push(entry);
        }
        Ok(())
    }

    pub(super) fn continuous_applies(&self, entry: &Value, id: &str) -> bool {
        list(&entry["applies_to"]).contains(&json!(id))
            && self.state.objects.get(id).is_none_or(|object| {
                entry["generations"][id]
                    .as_u64()
                    .or_else(|| entry["generation"].as_u64())
                    == Some(object.generation)
            })
    }

    fn reveal_until(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let seat = self
            .seats(string(&node["side"]), frame)
            .into_iter()
            .next()
            .unwrap_or_default();
        let wanted = self.number(&node["count"], frame)?.max(0);
        let mut count = 0_i64;
        let mut revealed = Vec::new();
        let qualifies: BTreeSet<_> = self
            .select(&node["qualifies"], frame)?
            .into_iter()
            .collect();
        for value in self.zone(&seat, "deck") {
            if count >= wanted {
                break;
            }
            let id = value.as_str().ok_or_else(|| {
                EngineFailure::Unsupported("unidentified filler reached by reveal-until".into())
            })?;
            revealed.push(id.to_owned());
            if qualifies.contains(id) {
                count = count.saturating_add(1);
            }
        }
        self.reveal(&revealed, "all", frame, false);
        frame
            .bindings
            .insert(string(&node["bind"]).into(), revealed);
        Ok(())
    }
}
