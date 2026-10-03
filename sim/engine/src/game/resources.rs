#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON has total read indexing and constructed write maps."
)]
use super::{EventOccurrence, Frame, Game, int, list, string};
use crate::{Result, invalid};
use serde_json::{Value, json};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Resource rules share the authoritative game state."
)]
impl Game {
    pub(super) fn change_counters(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let amount = self.number(&node["amount"], frame)?;
        let name = string(&node["name"]);
        let group = self.group();
        frame.performed = 0;
        for id in self.select(&node["subjects"], frame)? {
            let before = int(&self.object(&id)?.state["counters"][name]);
            let after = before.saturating_add(amount).max(0);
            let delta = after.saturating_sub(before);
            if delta == 0 {
                continue;
            }
            self.object_mut(&id)?.state["counters"][name] = json!(after);
            frame.performed = frame.performed.saturating_add(delta.saturating_abs());
            self.emit(
                json!({"kind":"カウンター","object":id,"name":self.catalog.keyword_name(name),"delta":delta}),
                &frame.cause,
                group,
            );
        }
        Ok(())
    }

    pub(super) fn import_event_occurrences(&mut self, semantic: &Value) -> Result<()> {
        for value in list(&semantic["event_occurrences"]) {
            let entry: EventOccurrence = serde_json::from_value(value).map_err(invalid)?;
            self.object(&entry.subject)?;
            let key = json!([entry.event, entry.subject]).to_string();
            if entry.count == 0 || self.state.event_occurrences.insert(key, entry).is_some() {
                return Err(invalid("event occurrences must be unique positive records"));
            }
        }
        Ok(())
    }

    fn next_occurrence(&mut self, event: &str, subject: &str) -> u64 {
        let key = json!([event, subject]).to_string();
        let entry = self
            .state
            .event_occurrences
            .entry(key)
            .or_insert_with(|| EventOccurrence {
                event: event.into(),
                subject: subject.into(),
                count: 0,
            });
        entry.count = entry.count.saturating_add(1);
        entry.count
    }

    fn race(&mut self, node: &Value, frame: &Frame) -> Result<()> {
        let count = self.number(&node["count"], frame)?.max(0);
        if count > 10_000 {
            return Err(crate::EngineFailure::Unsupported(
                "resource event count exceeds prototype fuel".into(),
            ));
        }
        let ids = self.select(&node["subjects"], frame)?;
        let mut pending = Vec::new();
        for _ in 0..count {
            for id in &ids {
                let n = self.next_occurrence("race", id);
                pending.extend(self.collect_event(
                    "race",
                    &[self.object(id)?.clone()],
                    &frame.cause,
                    &json!({"n":n}),
                )?);
            }
        }
        self.enqueue(pending);
        Ok(())
    }

    pub(super) fn recover_pp(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        frame.performed = 0;
        for seat in self.seats(string(&node["side"]), frame) {
            let amount = self.number(&node["amount"], frame)?.max(0);
            let player = self.player_mut(&seat)?;
            let old = int(&player.pp["current"]);
            let recovered = amount.min(int(&player.pp["max"]).saturating_sub(old).max(0));
            if recovered == 0 {
                continue;
            }
            player.pp["current"] = json!(old.saturating_add(recovered));
            frame.performed = frame.performed.saturating_add(recovered);
            let group = self.group();
            self.emit(
                json!({"kind":"回復","player":seat,"amount":recovered}),
                &frame.cause,
                group,
            );
        }
        Ok(())
    }

    pub(super) fn change_player_resource(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let amount = self.number(&node["amount"], frame)?;
        let maximum = node["op"] == "max_pp";
        let group = self.group();
        frame.performed = 0;
        for seat in self.seats(string(&node["side"]), frame) {
            let player = self.player_mut(&seat)?;
            let before = if maximum {
                int(&player.pp["max"])
            } else {
                player.ep
            };
            let after = if maximum {
                before.saturating_add(amount).clamp(0, 10)
            } else {
                before
                    .checked_add(amount)
                    .ok_or_else(|| {
                        crate::EngineFailure::Unsupported(
                            "EP value exceeds the integer representation".into(),
                        )
                    })?
                    .max(0)
            };
            if maximum {
                player.pp["max"] = json!(after);
                player.pp["current"] = json!(int(&player.pp["current"]).clamp(0, after));
            } else {
                player.ep = after;
            }
            let delta = after.saturating_sub(before);
            if delta != 0 {
                frame.performed = frame.performed.saturating_add(delta.saturating_abs());
                self.emit(json!({"kind":if maximum {"PP最大値変化"} else {"EP変化"},"player":seat,"source":frame.source,"delta":delta,"before":before,"after":after}), &frame.cause, group);
            }
        }
        Ok(())
    }

    pub(super) fn resource_code(&self, mut code: Value) -> Value {
        let keyword = string(&code["body"]["name"]).to_owned();
        if code["body"]["op"] == "keyword"
            && matches!(keyword.as_str(), "single_drive" | "twin_drive")
        {
            code["kind"] = json!("trigger");
            code["event"] = json!("attack");
            code["subject"] = json!("self");
            code["keyword"] = json!(keyword);
            code["rule"] = json!(if keyword == "twin_drive" {
                "14.4.6.3"
            } else {
                "14.4.6.2"
            });
            code["body"] =
                json!({"op":"drive","count":if keyword == "twin_drive" {2_i64} else {1_i64}});
        }
        let mut costs = list(&code["costs"]);
        if code["kind"] == "ride" {
            self.expand_ride_payment(&mut costs);
        }
        let mut specs = list(&code["cost_selections"]);
        for cost in &mut costs {
            let resource = match string(&cost["op"]) {
                "lesson" => Some(("ex", "lesson_item")),
                "eat" => Some(("evolve_deck", "meal_item")),
                "_drive_point" => Some(("evolve_deck", "drive_point")),
                _ => None,
            };
            if let Some((zone, role)) = resource {
                let key = specs.len().saturating_add(1).to_string();
                let count = cost.get("count").cloned().unwrap_or_else(|| json!(1_i64));
                let mut selector = json!({"side":"self","zone":zone,"resource_role":role});
                if zone == "evolve_deck" {
                    selector["where"] = json!({"fn":"ne","args":[{"read":"item.face_up"},true]});
                }
                specs.push(json!({"key":key,"select":selector,"min":count,"max":count}));
                cost["subjects"] = json!(format!("cost.{key}"));
            }
        }
        if !specs.is_empty() {
            code["cost_selections"] = json!(specs);
        }
        if !costs.is_empty() {
            code["costs"] = json!(costs);
        }
        code
    }

    fn expand_ride_payment(&self, costs: &mut Vec<Value>) {
        let explicit = costs
            .iter()
            .filter(|cost| cost["op"] == "link_resource")
            .count();
        if explicit == 0 {
            costs.push(json!({"op":"_drive_point"}));
            return;
        }
        if explicit != 1 {
            return;
        }
        for cost in costs {
            if cost["op"] == "link_resource"
                && cost["subjects"] == "self"
                && (cost["resource_role"] == "drive_point"
                    || cost["name"].as_str().is_some_and(|name| {
                        self.catalog.rule_bindings.legacy_resource(name) == Some("drive_point")
                    }))
                && cost["count"] == 1_i64
                && cost["from_zone"] == "evolve_deck"
                && cost["to"] == "drive"
            {
                *cost = json!({"op":"_drive_point"});
            }
        }
    }

    pub(super) fn cost_atoms(
        &self,
        costs: &[Value],
        frame: &Frame,
        out: &mut Vec<(Value, Frame)>,
    ) -> Result<()> {
        for cost in costs {
            match string(&cost["op"]) {
                "seq" => self.cost_atoms(&list(&cost["steps"]), frame, out)?,
                "if" => {
                    let branch = if self.truth(&cost["condition"], frame)? {
                        "then"
                    } else {
                        "else"
                    };
                    if !cost[branch].is_null() {
                        self.cost_atoms(&[cost[branch].clone()], frame, out)?;
                    }
                }
                "for_each" => {
                    for id in self.select(&cost["select"], frame)? {
                        let mut context = frame.clone();
                        context
                            .bindings
                            .insert(string(&cost["bind"]).into(), vec![id]);
                        self.cost_atoms(&[cost["body"].clone()], &context, out)?;
                    }
                }
                _ => out.push((cost.clone(), frame.clone())),
            }
        }
        Ok(())
    }

    pub(super) fn playable_zone(&self, id: &str) -> Result<bool> {
        let zone = &self.object(id)?.zone;
        if matches!(zone.as_str(), "hand" | "ex") {
            return Ok(true);
        }
        for code in self.abilities(id)? {
            let node = &code["body"];
            if code["kind"] == "static"
                && node["op"] == "play_permission"
                && list(&node["from"]).contains(&json!(zone))
                && node.get("condition").map_or(Ok(true), |condition| {
                    self.truth(condition, &self.frame_for(id)?)
                })?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn unlimited_evolution(&self, seat: &str) -> Result<bool> {
        for id in self.zone_ids(seat, "field") {
            for code in self.abilities(&id)? {
                if code["kind"] == "static"
                    && code["body"]["op"] == "rule_override"
                    && code["body"]["rule"] == "evolve_per_turn"
                    && code["body"]["value"] == "unlimited"
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub(super) fn resource_options(&self, id: &str, code: &Value) -> Result<Vec<Value>> {
        let frame = self.frame_for(id)?;
        let amount = list(&code["costs"])
            .iter()
            .filter(|cost| cost["op"] == "pp")
            .map(|cost| self.number(&cost["amount"], &frame))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .sum::<i64>();
        let mut result = Vec::new();
        for ep in 0..=self.player(&frame.controller)?.ep.min(1).min(amount) {
            let pp = amount.saturating_sub(ep);
            let mut candidate = code.clone();
            let mut costs = list(&code["costs"])
                .into_iter()
                .filter(|cost| cost["op"] != "pp")
                .collect::<Vec<_>>();
            costs.push(json!({"op":"pp","amount":pp}));
            candidate["costs"] = json!(costs);
            for option in self.parameterize(
                json!({"do":"activate","ability":self.reference(id,code),"pay":{"pp":pp,"ep":ep}}),
                &candidate,
                &frame,
            )? {
                let mut check = option.clone();
                check["by"] = json!(frame.controller);
                check["at"] = json!("main");
                if self.clone().resource_activation(&check, code)? {
                    result.push(option);
                }
            }
        }
        Ok(result)
    }

    pub(super) fn resource_activation(&mut self, decision: &Value, code: &Value) -> Result<bool> {
        let source = string(&decision["ability"]["source"]);
        let seat = self.object(source)?.controller.clone();
        if seat != decision["by"]
            || !self.ability_zone(source, code)?
            || decision["at"] != "main"
            || self
                .state
                .counters
                .get(&format!("{seat}.evolve_played"))
                .copied()
                .unwrap_or_default()
                > 0
            || (code["kind"] == "ride" && self.object(source)?.state["ride_used"] == true)
            || (code["kind"] == "meal"
                && !list(&self.object(source)?.state["links"]["出走"]).is_empty())
        {
            return Ok(false);
        }
        let mut frame = self.start_frame(source, self.reference(source, code), decision)?;
        let amount = list(&code["costs"])
            .iter()
            .filter(|cost| cost["op"] == "pp")
            .map(|cost| self.number(&cost["amount"], &frame))
            .collect::<Result<Vec<_>>>()?
            .iter()
            .sum::<i64>();
        let ep = int(&decision["pay"]["ep"]);
        let pp = decision["pay"].get("pp").map_or(amount, int);
        if !(0..=1).contains(&ep)
            || pp < 0
            || pp.saturating_add(ep) != amount
            || self.player(&seat)?.ep < ep
        {
            return Ok(false);
        }
        let mut paid_code = code.clone();
        let mut costs = list(&code["costs"])
            .into_iter()
            .filter(|cost| cost["op"] != "pp")
            .collect::<Vec<_>>();
        costs.push(json!({"op":"pp","amount":pp}));
        paid_code["costs"] = json!(costs);
        if !self.valid_parameters(&paid_code, &frame)?
            || !self.can_pay(&list(&paid_code["costs"]), &frame)?
        {
            return Ok(false);
        }
        self.player_mut(&seat)?.ep = self.player(&seat)?.ep.saturating_sub(ep);
        self.pay_costs(&paid_code, &mut frame)?;
        self.bump(&format!("{seat}.evolve_played"), 1);
        if code["kind"] == "ride" {
            self.object_mut(source)?.state["ride_used"] = json!(true);
        }
        self.begin_ability(&mut frame, code)?;
        Ok(true)
    }

    pub(super) fn resource_effect(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        match string(&node["op"]) {
            "lesson" => {
                // A lesson banishes, so it goes through the same path as `op: banish`
                // (消滅 events and banish triggers).
                let ids = self.select(&node["subjects"], frame)?;
                self.banish_objects(&ids, frame)?;
            }
            "eat" | "_drive_point" => {
                let ids = self.select(&node["subjects"], frame)?;
                let (zone, link) = if node["op"] == "eat" {
                    ("race", "出走")
                } else {
                    ("drive", "憑依")
                };
                self.move_objects(&ids, zone, None, None, frame)?;
                self.object_mut(&frame.source)?.state["links"][link] = json!(ids);
            }
            "race" => self.race(node, frame)?,
            "gain_drive" => self.gain_drive(node, frame)?,
            "stack" => self.increase_stack(node, frame)?,
            _ => return Err(invalid("unknown resource effect")),
        }
        Ok(())
    }

    fn gain_drive(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        frame.performed = 0;
        let mut gained = Vec::new();
        for id in self.select(&node["subjects"], frame)? {
            if self.object(&id)?.zone != "field" || !self.object_type(&id)?.contains("フォロワー")
            {
                continue;
            }
            if self.object(&id)?.state["drive_gained"] == true {
                continue;
            }
            self.object_mut(&id)?.state["drive_gained"] = json!(true);
            if self.keywords(&id)?.contains("drive") {
                continue;
            }
            let abilities: Vec<_> = ["drive", "single_drive", "rush"]
                .into_iter()
                .map(|keyword| json!({"kind":"static","body":{"op":"keyword","name":keyword}}))
                .collect();
            self.state.continuous.push(json!({"source":id,"reference":{"source":id,"rule":"14.4.7.3.2"},"applies_to":[id],"generation":self.object(&id)?.generation,"effect":{"op":"modify","abilities":abilities},"until":"game","order":self.state.next_event}));
            gained.push(self.object(&id)?.clone());
            frame.performed = frame.performed.saturating_add(1);
        }
        let pending = self.collect_triggers("gain_drive", &gained, &frame.cause)?;
        self.enqueue(pending);
        Ok(())
    }

    fn increase_stack(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let amount = self.number(&node["amount"], frame)?;
        if amount <= 0 {
            frame.performed = 0;
            return Ok(());
        }
        let selector = json!({"side":"self","zone":"field","keyword":"stack"});
        if self.select(&selector, frame)?.is_empty() {
            let id = self.new_resource_object("stack_base", &frame.controller)?;
            frame
                .values
                .insert("stack_entry_counts".into(), json!({&id:amount}));
            Self::prepend(frame, vec![json!({"op":"move","subjects":id,"to":"field"})]);
        } else {
            Self::prepend(
                frame,
                vec![
                    json!({"op":"select","select":selector,"min":1_i64,"max":1_i64,"bind":"stack-recipient"}),
                    json!({"op":"counter","subjects":"stack-recipient","name":"stack_counter","amount":amount}),
                ],
            );
        }
        Ok(())
    }

    pub(super) fn initialize_entry_counters(&mut self, id: &str, frame: &Frame) -> Result<()> {
        if self.keywords(id)?.contains("stack") {
            let count = frame
                .values
                .get("stack_entry_counts")
                .and_then(|counts| counts.get(id))
                .map_or(1, int);
            let old = int(&self.object(id)?.state["counters"]["stack_counter"]);
            self.object_mut(id)?.state["counters"]["stack_counter"] =
                json!(old.saturating_add(count));
        }
        Ok(())
    }
}
