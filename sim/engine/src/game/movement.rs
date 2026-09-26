#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use super::{Frame, Game, Object, list, scalar, string};
use crate::Result;
use alloc::collections::{BTreeMap, BTreeSet};
use core::slice::from_ref;
use serde_json::{Value, json};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn emit_zone_movement(
        &mut self,
        previous: &Object,
        destination: &str,
        position: Option<&Value>,
        frame: &Frame,
        group: u64,
    ) -> Result<()> {
        let id = &previous.id;
        let owner = &self.object(id)?.controller;
        let from = format!("{}.{}", previous.controller, previous.zone);
        let mut event =
            json!({"kind":"移動","object":id,"from":from,"to":format!("{owner}.{destination}")});
        if previous.zone == "void"
            && matches!(destination, "ex" | "equipment" | "cemetery" | "banish")
        {
            if previous.state["creation_print_selected"] == true {
                event["card"] = json!(previous.card);
            }
            event["source"] = json!(frame.source);
        }
        if let Some(pos) = position
            && destination == "deck"
        {
            event["position"] = json!(pos);
        }
        if let Some(rule) = frame.values.get("movement_rule") {
            event["by"] = rule.clone();
        }
        let cause = frame
            .values
            .get("movement_rules")
            .and_then(|rules| rules.get(id))
            .map_or_else(
                || frame.cause.clone(),
                |rule| {
                    event["by"] = json!(format!("rule-{}", string(rule)));
                    json!({"rule":rule})
                },
            );
        self.emit(event, &cause, group);
        Ok(())
    }

    pub(super) fn change_posture(
        &mut self,
        ids: &[String],
        acted: bool,
        cause: &Value,
    ) -> Result<Vec<String>> {
        let group = self.group();
        let mut changed = Vec::new();
        for id in ids {
            if self.object(id)?.state["acted"] != acted {
                self.object_mut(id)?.state["acted"] = json!(acted);
                changed.push(self.object(id)?.clone());
                self.emit(
                    json!({"kind":if acted {"アクト"} else {"スタンド"},"object":id}),
                    cause,
                    group,
                );
            }
        }
        let pending =
            self.collect_triggers(if acted { "act" } else { "stand" }, &changed, cause)?;
        self.enqueue(pending);
        Ok(changed.into_iter().map(|object| object.id).collect())
    }

    pub(super) fn emit_banish_instruction(
        &mut self,
        movable: &[String],
        destinations: &BTreeMap<String, String>,
        frame: &Frame,
    ) -> Result<()> {
        if frame.values.get("banish_instruction") == Some(&json!(true)) {
            let group = self.group();
            for id in movable {
                if destinations
                    .get(id)
                    .is_some_and(|destination| destination == "banish")
                    && !matches!(self.object(id)?.zone.as_str(), "banish" | "void")
                {
                    self.emit(
                        json!({"kind":"消滅","object":id,"source":frame.source}),
                        &frame.cause,
                        group,
                    );
                }
            }
        }
        Ok(())
    }

    pub(super) fn banish_objects(&mut self, ids: &[String], frame: &Frame) -> Result<()> {
        let mut movement = frame.clone();
        movement
            .values
            .insert("banish_instruction".into(), json!(true));
        self.move_objects(ids, "banish", None, None, &movement)
    }

    pub(super) fn discard(&mut self, ids: &[String], frame: &Frame) -> Result<()> {
        let discarded = ids
            .iter()
            .filter_map(|id| self.state.objects.get(id))
            .filter(|object| object.zone == "hand")
            .cloned()
            .collect::<Vec<_>>();
        let group = self.group();
        for object in &discarded {
            self.bump(&format!("{}.discarded", object.controller), 1);
            self.emit(
                json!({"kind":"捨てる","player":object.controller,"object":object.id}),
                &frame.cause,
                group,
            );
        }
        let pending = self.collect_triggers("discard", &discarded, &frame.cause)?;
        let actual = discarded
            .iter()
            .map(|object| object.id.clone())
            .collect::<Vec<_>>();
        self.move_objects(&actual, "cemetery", None, None, frame)?;
        self.enqueue(pending);
        Ok(())
    }

    pub(super) fn moved_attributes(previous: &Object, printed: &Value, destination: &str) -> Value {
        if matches!(previous.zone.as_str(), "ex" | "resolution")
            && matches!(destination, "resolution" | "field")
        {
            return previous.state.clone();
        }
        let acted = destination == "field" && previous.state["acted"] == true;
        json!({"power":scalar(&printed["power"]),"hp":scalar(&printed["hp"]),"max_hp":scalar(&printed["hp"]),"acted":acted,"evolved":false,"entered_this_turn":false,"face":0_i64,"damage":0_i64,"counters":{},"keywords":[],"silenced":false,"stats_increased_this_turn":false,"attacks_this_turn":0_i64})
    }

    pub(super) fn movable_subjects(&self, node: &Value, frame: &Frame) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for id in self.select(&node["subjects"], frame)? {
            if node["op"] != "banish" || !self.restricted(&id, "banish")? {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    pub(super) fn card_name(&self, id: &str) -> Result<String> {
        let object = self.object(id)?;
        Ok(self
            .catalog
            .programs
            .get(&object.card)
            .and_then(|program| program["rules_name"].as_str())
            .unwrap_or_else(|| self.face(id).ok().map_or("", |face| string(&face["name"])))
            .into())
    }

    pub(super) fn movement_destinations(
        &mut self,
        ids: &[String],
        zone: &str,
        frame: &Frame,
    ) -> Result<BTreeMap<String, String>> {
        let mut destinations = BTreeMap::new();
        let group = self.group();
        for id in ids {
            let mut destination = zone.to_owned();
            for source in self.field_ids() {
                for code in self.abilities(&source)? {
                    let body = &code["body"];
                    if body["op"] != "replace_move"
                        || body["from"] != self.object(id)?.zone
                        || body["to"] != zone
                    {
                        continue;
                    }
                    let context = self.frame_for(&source)?;
                    if !self.matches(id, &body["subjects"], &context)? {
                        continue;
                    }
                    if body["replacement"] == "banish" && self.restricted(id, "banish")? {
                        continue;
                    }
                    destination = string(&body["replacement"]).into();
                    break;
                }
                if destination != zone {
                    break;
                }
            }
            if destination != zone {
                self.emit(json!({"kind":"取代","object":id,"original":if frame.values.get("paying_cost") == Some(&json!(true)) && id == &frame.source {"これを墓場に置く"} else {"場から墓場に置く"},"replacement":"消滅させる"}),&frame.cause,group);
                if destination == "banish" {
                    self.emit(json!({"kind":"消滅","object":id}), &frame.cause, group);
                }
            }
            destinations.insert(id.clone(), destination);
        }
        Ok(destinations)
    }

    pub(super) fn release_links(&mut self, leaving: &[Object], frame: &Frame) -> Result<()> {
        let mut resources = BTreeSet::new();
        let mut equipment = BTreeSet::new();
        for object in leaving {
            if let Some(evolve) = object.state["evolved_with"].as_str() {
                resources.insert(evolve.to_owned());
            }
            if let Some(links) = object.state["links"].as_object() {
                for linked_ids in links.values() {
                    resources.extend(
                        list(linked_ids)
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned),
                    );
                }
            }
            equipment.extend(
                self.state
                    .objects
                    .values()
                    .filter(|item| {
                        item.zone == "equipment" && item.state["equipped_to"] == object.id
                    })
                    .map(|item| item.id.clone()),
            );
        }
        for id in resources {
            let rule = match self.object(&id)?.zone.as_str() {
                "evolution" => "11.6.1",
                "race" => "11.8.1",
                "drive" => "11.10.1",
                _ => continue,
            };
            let mut rule_frame = frame.clone();
            rule_frame.cause = json!({"rule":rule});
            rule_frame
                .values
                .insert("movement_rule".into(), json!(format!("rule-{rule}")));
            self.move_objects(from_ref(&id), "evolve_deck", None, None, &rule_frame)?;
            self.object_mut(&id)?.state["face_up"] = json!(true);
        }
        for id in equipment {
            let object = self.object(&id)?.clone();
            self.player_mut(&object.controller)?
                .zones
                .entry("equipment".into())
                .or_default()
                .retain(|value| *value != id);
            self.object_mut(&id)?.zone = "void".into();
            let group = self.group();
            self.emit(
                json!({"kind":"消去","object":id,"by":"rule-11.11.1"}),
                &json!({"rule":"11.11.1"}),
                group,
            );
        }
        Ok(())
    }
}
