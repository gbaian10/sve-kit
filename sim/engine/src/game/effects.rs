#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use super::legal::permutations;
use crate::game::legal::subsets;
use core::slice::from_ref;
use serde_json::{Value, json};

use super::{Frame, Game, Prompt, int, list, other, string};
use crate::{EngineFailure, Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn run_frame(&mut self, mut frame: Frame) -> Result<()> {
        let mut fuel = 20_000_u32;
        while self.state.prompt.is_none()
            && !frame.todo.is_empty()
            && self.state.game["ended"] != true
        {
            fuel = fuel.checked_sub(1).ok_or_else(|| {
                EngineFailure::Unsupported(
                    "resolution fuel exhausted; possible permanent loop".into(),
                )
            })?;
            let task = frame.todo.remove(0);
            self.execute(&task, &mut frame)?;
        }
        if self.state.prompt.is_some() {
            self.state.frame = Some(frame);
        } else {
            self.state.frame = None;
        }
        Ok(())
    }

    pub(super) fn prepend(frame: &mut Frame, steps: Vec<Value>) {
        frame.todo.splice(0..0, steps);
    }
    pub(super) fn prompt(&mut self, frame: &mut Frame, choices: Vec<Value>, resume: Value) {
        frame.occurrence = frame.occurrence.saturating_add(1);
        self.state.prompt = Some(Prompt {
            by: frame.controller.clone(),
            choices,
            resume,
        });
    }

    #[expect(
        clippy::too_many_lines,
        reason = "The continuation dispatch keeps all serialized resume variants auditable together."
    )]
    pub(super) fn resume(&mut self, decision: &Value) -> Result<()> {
        let prompt = self
            .state
            .prompt
            .take()
            .ok_or_else(|| invalid("no suspended input"))?;
        let mut frame = self
            .state
            .frame
            .take()
            .ok_or_else(|| invalid("missing continuation"))?;
        let task = prompt.resume;
        match string(&task["resume"]) {
            "select" => {
                let selected = list(if task["order"] == true {
                    &decision["order"]
                } else {
                    &decision["select"]
                })
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
                frame
                    .bindings
                    .insert(string(&task["bind"]).into(), selected);
            }
            "declare" => {
                frame
                    .values
                    .insert(string(&task["bind"]).into(), decision["declare"].clone());
            }
            "choice" => {
                let mut selected = list(&decision["options"]);
                selected.sort_by_key(int);
                Self::prepend(
                    &mut frame,
                    selected
                        .iter()
                        .filter_map(|value| {
                            let index = usize::try_from(int(value).saturating_sub(1)).ok()?;
                            task["modes"].as_array()?.get(index).cloned()
                        })
                        .collect(),
                );
            }
            "optional" => {
                if decision["choice"] == "execute" {
                    Self::prepend(&mut frame, vec![task["then"].clone()]);
                }
            }
            "pay" => {
                if decision["choice"] == "execute" {
                    for cost in list(&task["costs"]) {
                        self.execute(&cost, &mut frame)?;
                    }
                    frame.paid = true;
                    Self::prepend(&mut frame, vec![task["then"].clone()]);
                }
                if decision["choice"] != "execute" && !task["else"].is_null() {
                    Self::prepend(&mut frame, vec![task["else"].clone()]);
                }
            }
            "place" => {
                let id = string(&task["object"]);
                self.object_mut(id)?.state["acted"] = decision["acted"].clone();
                self.move_objects(&[id.into()], "field", None, None, &frame)?;
            }
            "search" => {
                let selected = list(&decision["select"])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                self.reveal(&selected, other(&frame.controller), &frame, false);
                frame.bindings.insert("search-result".into(), selected);
                Self::prepend(
                    &mut frame,
                    vec![
                        json!({"op":"move","subjects":"search-result","to":task["to"],"bind":task["bind"],"suppress_fanfare":task["suppress_fanfare"]}),
                        json!({"op":"shuffle","subjects":{"zone":"deck","side":"self"}}),
                    ],
                );
            }
            "capacity" => {}
            "move-capacity" => {
                let selected = list(&decision["select"])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                frame.bindings.insert("capacity-selected".into(), selected);
                let mut movement = task["node"].clone();
                movement["subjects"] = json!("capacity-selected");
                movement["capacity_checked"] = json!(true);
                Self::prepend(&mut frame, vec![movement]);
            }
            "place-batch" => {
                let id = string(&decision["object"]);
                self.object_mut(id)?.state["acted"] = decision["acted"].clone();
                let mut movement = task["node"].clone();
                let mut placed = list(&movement["placed"]);
                placed.push(json!(id));
                movement["placed"] = json!(placed);
                Self::prepend(&mut frame, vec![movement]);
            }
            "drive" => {
                let id = string(&task["object"]);
                let execute = decision["choice"] == "execute";
                let mut steps = Vec::new();
                if execute {
                    steps.push(match string(&task["icon"]) {
                        "draw" => json!({"op":"draw","count":1_i64}),
                        "heal" => json!({"op":"modify","subjects":"self.leader","hp":3_i64}),
                        "stand" => {frame.bindings.insert("drive-target".into(),list(&decision["select"]).iter().map(|chosen|string(chosen).into()).collect());json!({"op":"seq","steps":[{"op":"stand","subjects":"drive-target"},{"op":"restrict","subjects":"drive-target","action":"attack_leader","until":"end-of-turn"}]})},
                        "critical" => {frame.bindings.insert("drive-target".into(),list(&decision["select"]).iter().map(|chosen|string(chosen).into()).collect());json!({"op":"modify","subjects":"drive-target","power":2_i64,"hp":2_i64})},
                        _ => return Err(invalid("unknown verified trigger icon"))
                    });
                }
                steps.push(json!({"op":"move","subjects":id,"to":if execute { "cemetery" } else { "deck" },"position":"bottom"}));
                if execute {
                    steps.push(json!({"op":"_drive_trigger"}));
                }
                Self::prepend(&mut frame, steps);
            }
            "nested" => self.start_nested(&task["node"], decision, &mut frame)?,
            "replacements" => {
                let key = string(&task["target"]).to_owned();
                let mut task = task["task"].clone();
                task["orders"][&key] = decision["order"].clone();
                Self::prepend(&mut frame, vec![task]);
            }
            unknown => return Err(invalid(format!("unknown continuation: {unknown}"))),
        }
        self.run_frame(frame)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "This is the single auditable dispatch table for schema atoms and combinators."
    )]
    pub(super) fn execute(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        Self::check_execution_parameters(node)?;
        match string(&node["op"]) {
            "seq" => Self::prepend(frame, list(&node["steps"])),
            "_reference" => frame.reference = node["reference"].clone(),
            "if" => {
                let branch = if self.truth(&node["condition"], frame)? {
                    &node["then"]
                } else {
                    &node["else"]
                };
                if !branch.is_null() {
                    Self::prepend(frame, vec![branch.clone()]);
                }
            }
            "per" => {
                let count =
                    usize::try_from(self.number(&node["count"], frame)?.max(0)).map_err(invalid)?;
                if count > 10_000 {
                    return Err(EngineFailure::Unsupported(
                        "repeat count exceeds prototype fuel".into(),
                    ));
                }
                Self::prepend(frame, vec![node["body"].clone(); count]);
            }
            "repeat" => Self::prepend(
                frame,
                vec![
                    node["body"].clone(),
                    json!({"op":"_repeat","body":node["body"],"until":node["until"]}),
                ],
            ),
            "_repeat" => {
                if !self.truth(&node["until"], frame)? {
                    Self::prepend(frame, vec![node["body"].clone(), node.clone()]);
                }
            }
            "for_each" => {
                let mut steps = Vec::new();
                for id in self.select(&node["select"], frame)? {
                    steps.push(json!({"op":"_bind","bind":node["bind"],"object":id}));
                    steps.push(node["body"].clone());
                }
                Self::prepend(frame, steps);
            }
            "_bind" => {
                frame.bindings.insert(
                    string(&node["bind"]).into(),
                    vec![string(&node["object"]).into()],
                );
            }
            "choice" if node["timing"] == "resolve" || node.get("by").is_some() => {
                self.resolution_choice(node, frame)?;
            }
            "declare_number" => self.prompt(
                frame,
                Vec::new(),
                json!({"resume":"declare","bind":node["bind"]}),
            ),
            "choice" => {
                let modes = list(&node["modes"]);
                let mut indexes = list(&frame.decision["options"])
                    .iter()
                    .filter_map(Value::as_u64)
                    .collect::<Vec<_>>();
                indexes.sort_unstable();
                let steps = indexes
                    .into_iter()
                    .filter_map(|n| {
                        let i = usize::try_from(n.saturating_sub(1)).ok()?;
                        modes.get(i).cloned()
                    })
                    .collect();
                Self::prepend(frame, steps);
            }
            "optional" => self.prompt(
                frame,
                vec![
                    json!({"do":"resolve-choice","choice":"execute"}),
                    json!({"do":"resolve-choice","choice":"decline"}),
                ],
                json!({"resume":"optional","then":node["then"]}),
            ),
            "pay" => {
                let mut choices = vec![json!({"do":"resolve-choice","choice":"decline"})];
                if self.can_pay(&list(&node["costs"]), frame)? {
                    choices.insert(0, json!({"do":"resolve-choice","choice":"execute"}));
                }
                self.prompt(frame,choices,json!({"resume":"pay","costs":node["costs"],"then":node["then"],"else":node["else"]}));
            }
            "if_done" => Self::prepend(
                frame,
                vec![
                    node["attempt"].clone(),
                    json!({"op":"_if_done","then":node["then"]}),
                ],
            ),
            "_if_done" => {
                if frame.performed > 0 {
                    Self::prepend(frame, vec![node["then"].clone()]);
                }
            }
            "select" => self.resolution_select(node, frame)?,
            "damage" => {
                let amount = self.number(&node["amount"], frame)?;
                let split = node["split"].as_str();
                let hits=self.select(&node["subjects"],frame)?.iter().map(|id|json!({"source":frame.source,"target":id,"amount":split.map_or(amount,|key|int(&frame.decision["distribute"][key][id])),"battle":false})).collect::<Vec<_>>();
                Self::prepend(
                    frame,
                    vec![json!({"op":"_damage","hits":hits,"orders":{},"bind":node["bind"]})],
                );
            }
            "_damage" => self.damage_batch(node, frame)?,
            "_movement_receipt" => {
                let mut moved = Vec::new();
                for previous in list(&node["before"]) {
                    let id = string(&previous["id"]);
                    if self.object(id)?.generation
                        != previous["generation"].as_u64().unwrap_or_default()
                    {
                        moved.push(id.to_owned());
                    }
                }
                frame.performed = i64::try_from(moved.len()).unwrap_or(i64::MAX);
                if let Some(name) = node["bind"].as_str() {
                    frame.bindings.insert(name.into(), moved);
                }
            }
            "destroy" | "banish" | "discard" | "move" => self.zone_action(node, frame)?,
            "act" | "stand" => {
                frame.performed = 0;
                let group = self.group();
                let mut changed = Vec::new();
                for id in self.select(&node["subjects"], frame)? {
                    let acted = node["op"] == "act";
                    if self.object(&id)?.state["acted"] != acted {
                        self.object_mut(&id)?.state["acted"] = json!(acted);
                        frame.performed = frame.performed.saturating_add(1);
                        changed.push(id.clone());
                        self.emit(
                            json!({"kind":if acted { "アクト" } else { "スタンド" },"object":id}),
                            &frame.cause,
                            group,
                        );
                    }
                }
                if let Some(name) = node["bind"].as_str() {
                    frame.bindings.insert(name.into(), changed);
                }
            }
            "draw" => {
                frame.performed = 0;
                let mut drawn = Vec::new();
                for seat in self.seats(string(&node["side"]), frame) {
                    let before = self.zone_ids(&seat, "hand");
                    let count = self.zone_count(&seat, "hand");
                    for _ in 0..self.number(&node["count"], frame)?.max(0) {
                        self.draw(&seat, frame)?;
                    }
                    frame.performed = frame
                        .performed
                        .saturating_add(self.zone_count(&seat, "hand").saturating_sub(count));
                    drawn.extend(
                        self.zone_ids(&seat, "hand")
                            .into_iter()
                            .filter(|id| !before.contains(id)),
                    );
                }
                if let Some(name) = node["bind"].as_str() {
                    frame.bindings.insert(name.into(), drawn);
                }
            }
            "look" => {
                let seat = self
                    .seats(string(&node["side"]), frame)
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                let count =
                    usize::try_from(self.number(&node["count"], frame)?.max(0)).map_err(invalid)?;
                let ids = self
                    .zone_ids(&seat, "deck")
                    .into_iter()
                    .take(count)
                    .collect::<Vec<_>>();
                for id in &ids {
                    self.learn(&frame.controller, id, true);
                }
                frame.bindings.insert(string(&node["bind"]).into(), ids);
            }
            "search" => {
                for id in self.zone_ids(&frame.controller, "deck") {
                    self.learn(&frame.controller, &id, false);
                }
                let mut ids = self.select(&node["select"], frame)?;
                ids.sort();
                let max =
                    usize::try_from(self.number(&node["max"], frame)?.max(0)).map_err(invalid)?;
                let min =
                    usize::try_from(self.number(&node["min"], frame)?.max(0)).map_err(invalid)?;
                let choices = subsets(&ids, min, max)
                    .into_iter()
                    .map(|ids| json!({"do":"resolve-choice","select":ids}))
                    .collect();
                self.prompt(
                    frame,
                    choices,
                    json!({"resume":"search","to":node["to"],"bind":node["bind"],"suppress_fanfare":node["suppress_fanfare"]}),
                );
            }
            "shuffle" => {
                let ids = self.select(&node["subjects"], frame)?;
                self.shuffle(&frame.controller, &ids)?;
                let shuffled = self
                    .zone_ids(&frame.controller, "deck")
                    .into_iter()
                    .filter(|id| ids.contains(id))
                    .collect::<Vec<_>>();
                for binding in frame.bindings.values_mut() {
                    let mut order = shuffled
                        .iter()
                        .filter(|id| binding.contains(id))
                        .cloned()
                        .collect::<Vec<_>>()
                        .into_iter();
                    for id in binding {
                        if ids.contains(id)
                            && let Some(next) = order.next()
                        {
                            *id = next;
                        }
                    }
                }
            }
            "reveal" => {
                let ids = self.select(&node["subjects"], frame)?;
                self.reveal(&ids, string(&node["to"]), frame, true);
            }
            "modify" => self.modify(node, frame)?,
            "pp" => {
                let amount = self.number(&node["amount"], frame)?.max(0);
                let player = self.player_mut(&frame.controller)?;
                player.pp["current"] = json!(int(&player.pp["current"]).saturating_sub(amount));
                frame.performed = amount;
            }
            "recover_pp" => self.recover_pp(node, frame)?,
            "lesson" | "eat" | "_drive_point" | "race" | "gain_drive" | "stack" => {
                self.resource_effect(node, frame)?;
            }
            "drive" => {
                for _ in 0..self.number(&node["count"], frame)?.max(0) {
                    frame.todo.insert(0, json!({"op":"_drive"}));
                }
            }
            "_earth_payment" | "extra_turn" => self.payment_effect(node, frame)?,
            "win" => self.win_by_effect(node, frame)?,
            "_drive" => self.drive(frame)?,
            "_drive_trigger" => {
                let pending = self.collect_triggers("drive_trigger", &[], &frame.cause)?;
                self.enqueue(pending);
            }
            "delay" => self.register_delay(node, frame)?,
            "_finish_card" => {
                let source = frame.source.clone();
                if self.object(&source)?.zone == "resolution" {
                    self.move_objects(from_ref(&source), "cemetery", None, None, frame)?;
                }
                let group = self.group();
                self.emit(json!({"kind":"解決","object":source}), &frame.cause, group);
            }
            "_finish_ability" => {
                let group = self.group();
                self.emit(
                    json!({"kind":"解決","ability":frame.reference}),
                    &frame.cause,
                    group,
                );
            }
            "flip" | "control" | "create" | "counter" | "adjust_cost" | "restrict"
            | "replace_damage" | "reveal_until" | "box" | "equip" | "pilot" | "become_type" => {
                self.extended_effect(node, frame)?;
            }
            "play_card" | "play_ability" => self.nested_play(node, frame)?,
            "_nested_prepare" => self.prepare_nested(node, frame)?,
            "_restore_frame" => Self::restore_frame(node, frame)?,
            "unsupported" => {
                return Err(EngineFailure::Unsupported(string(&node["reason"]).into()));
            }
            unknown => {
                return Err(EngineFailure::Unsupported(format!(
                    "opcode not executable yet: {unknown}"
                )));
            }
        }
        Ok(())
    }

    fn zone_action(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let ids = self.movable_subjects(node, frame)?;
        let before = ids
            .iter()
            .map(|id| {
                self.object(id)
                    .map(|object| json!({"id":id,"generation":object.generation}))
            })
            .collect::<Result<Vec<_>>>()?;
        Self::prepend(
            frame,
            vec![json!({"op":"_movement_receipt","before":before,"bind":node["bind"]})],
        );
        match string(&node["op"]) {
            "destroy" => self.destroy(&ids, Some(frame))?,
            "banish" => {
                let group = self.group();
                for id in &ids {
                    self.emit(
                        json!({"kind":"消滅","object":id,"source":frame.source}),
                        &frame.cause,
                        group,
                    );
                }
                self.move_objects(&ids, "banish", None, None, frame)?;
            }
            "discard" => self.discard(&ids, frame)?,
            _ => {
                let zone = string(&node["to"]);
                if ids.is_empty() {
                    return Ok(());
                }
                if matches!(zone, "field" | "ex") && node["capacity_checked"] != true {
                    let available = usize::try_from(
                        5_i64
                            .saturating_sub(self.zone_count(&frame.controller, zone))
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
                }
                if zone == "field" {
                    for id in &ids {
                        if self.keywords(id)?.contains("guard")
                            && !list(&node["placed"]).contains(&json!(id))
                        {
                            self.prompt(
                                frame,
                                vec![
                                    json!({"do":"place-acted","object":id,"acted":false}),
                                    json!({"do":"place-acted","object":id,"acted":true}),
                                ],
                                json!({"resume":"place-batch","node":node}),
                            );
                            return Ok(());
                        }
                    }
                }
                let mut movement_frame = frame.clone();
                movement_frame
                    .values
                    .insert("suppress_fanfare".into(), node["suppress_fanfare"].clone());
                self.move_objects(
                    &ids,
                    zone,
                    node["side"].as_str(),
                    node.get("position"),
                    &movement_frame,
                )?;
            }
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Simultaneous leave snapshots, moves, token rules and entry triggers share one transaction."
    )]
    pub(super) fn move_objects(
        &mut self,
        ids: &[String],
        zone: &str,
        side: Option<&str>,
        position: Option<&Value>,
        frame: &Frame,
    ) -> Result<()> {
        let mut movable = Vec::new();
        for id in ids {
            let object = self.object(id)?;
            if object.zone == "field"
                && zone != "field"
                && self.keywords(id)?.contains("stack")
                && int(&object.state["counters"]["stack_counter"]) > 0
            {
                let count = int(&object.state["counters"]["stack_counter"]);
                self.object_mut(id)?.state["counters"]["stack_counter"] =
                    json!(count.saturating_sub(1));
                let group = self.group();
                self.emit(json!({"kind":"取代","object":id,"original":"場を離れる（墓場に置く）","replacement":"スタックカウンターを1個取り除き、場に残る"}),&frame.cause,group);
                self.emit(
                    json!({"kind":"カウンター","object":id,"name":self.catalog.keyword_name("stack_counter"),"delta":-1_i64}),
                    &frame.cause,
                    group,
                );
            } else {
                movable.push(id.clone());
            }
        }
        let destinations = self.movement_destinations(&movable, zone, frame)?;
        let leaving = movable
            .iter()
            .filter_map(|id| self.state.objects.get(id))
            .filter(|o| o.zone == "field" && zone != "field")
            .cloned()
            .collect::<Vec<_>>();
        let mut pending = self.collect_triggers("leave", &leaving, &frame.cause)?;
        pending.extend(self.collect_delayed("leave", &leaving, &frame.cause)?);
        let buried = leaving
            .iter()
            .filter(|object| {
                destinations
                    .get(&object.id)
                    .is_some_and(|destination| destination == "cemetery")
            })
            .cloned()
            .collect::<Vec<_>>();
        pending.extend(self.collect_triggers("field_to_cemetery", &buried, &frame.cause)?);
        pending.extend(self.collect_delayed("field_to_cemetery", &buried, &frame.cause)?);
        let group = self.group();
        let mut entered = Vec::new();
        let mut erased = Vec::new();
        for id in &movable {
            let destination = destinations.get(id).map_or(zone, String::as_str);
            let previous = self.object(id)?.clone();
            if previous.zone == destination && position.is_none() {
                continue;
            }
            let owner = if matches!(destination, "field" | "ex") {
                side.map_or_else(
                    || previous.controller.clone(),
                    |s| self.seats(s, frame).into_iter().next().unwrap_or_default(),
                )
            } else {
                previous.owner.clone()
            };
            if let Some(items) = self
                .player_mut(&previous.controller)?
                .zones
                .get_mut(&previous.zone)
            {
                items.retain(|v| v.as_str() != Some(id));
            }
            let items = self
                .player_mut(&owner)?
                .zones
                .entry(destination.into())
                .or_default();
            if let Some(position) = position.filter(|pos| **pos != "bottom") {
                let index = position.as_u64().unwrap_or(1).saturating_sub(1);
                items.insert(
                    usize::try_from(index)
                        .unwrap_or(usize::MAX)
                        .min(items.len()),
                    json!(id),
                );
            } else {
                items.push(json!(id));
            }
            let printed = self.catalog.face(&previous.card, 0)?.clone();
            let object = self.object_mut(id)?;
            object.zone = destination.into();
            object.controller.clone_from(&owner);
            object.generation = object.generation.saturating_add(1);
            object.state = Self::moved_attributes(&previous, &printed, destination);
            let from = format!("{}.{}", previous.controller, previous.zone);
            if destination == "field" {
                object.state["entered_this_turn"] = json!(true);
                object.state["entered_from"] = if previous.zone == "resolution" {
                    previous.state["entered_from"].clone()
                } else {
                    json!(previous.zone)
                };
                object.state["entered_by"] = json!(if previous.zone == "resolution" {
                    "play"
                } else {
                    "effect"
                });
                let event_id=self.emit(json!({"kind":"場に出す","object":id,"card":previous.card,"from":from,"to":format!("{owner}.field")}),&frame.cause,group);
                entered.push((self.object(id)?.clone(), json!({"event":event_id})));
            } else {
                let mut event = json!({"kind":"移動","object":id,"from":from,"to":format!("{owner}.{destination}")});
                if let Some(pos) = position
                    && destination == "deck"
                {
                    event["position"] = json!(pos);
                }
                if let Some(rule) = frame.values.get("movement_rule") {
                    event["by"] = rule.clone();
                }
                self.emit(event, &frame.cause, group);
            }
            for viewer in ["P1", "P2"] {
                let was_known = self
                    .state
                    .knowledge
                    .get(viewer)
                    .is_some_and(|k| k.located.contains(id));
                if !matches!(destination, "hand" | "deck" | "evolve_deck")
                    || (owner == viewer && destination == "hand")
                    || was_known
                {
                    self.learn(viewer, id, true);
                }
            }
            if previous.zone == "ex"
                && destination == "banish"
                && self.card_name(id)? == "魔法のアイテム"
            {
                self.bump(&format!("{}.magic_item_banished", previous.controller), 1);
            }
            let token = string(&printed["card_type"]).contains("トークン");
            if token && !matches!(destination, "field" | "ex" | "resolution" | "equipment") {
                erased.push(id.clone());
            }
        }
        let erase_group = self.group();
        for id in erased {
            let object = self.object(&id)?.clone();
            if let Some(items) = self
                .player_mut(&object.controller)?
                .zones
                .get_mut(&object.zone)
            {
                items.retain(|v| v.as_str() != Some(&id));
            }
            self.object_mut(&id)?.zone = "void".into();
            self.emit(
                json!({"kind":"消去","object":id,"by":"rule-9.1.4.4"}),
                &json!({"rule":"9.1.4.4"}),
                erase_group,
            );
        }
        self.release_links(&leaving, frame)?;
        self.enqueue(pending);
        for (object, _) in &entered {
            self.initialize_entry_counters(&object.id, frame)?;
        }
        for (object, cause) in entered {
            self.enter_triggers(&self.object(&object.id)?.clone(), &cause, frame)?;
        }
        Ok(())
    }

    pub(super) fn destroy(&mut self, ids: &[String], frame: Option<&Frame>) -> Result<()> {
        let mut actual = Vec::new();
        for id in ids {
            if self.object(id)?.zone != "field" {
                continue;
            }
            if frame.is_some() && self.restricted(id, "ability_destroy")? {
                continue;
            }
            actual.push(id.clone());
        }
        let cause = frame.map_or_else(|| json!({"rule":"11.3.1"}), |f| f.cause.clone());
        let group = self.group();
        for id in &actual {
            let mut event = json!({"kind":"破壊","object":id});
            if let Some(f) = frame {
                event["source"] = json!(f.source);
            } else {
                event["by"] = json!("rule-11.3.1");
            }
            self.emit(event, &cause, group);
        }
        let default = Frame {
            cause,
            ..Frame::default()
        };
        self.move_objects(&actual, "cemetery", None, None, frame.unwrap_or(&default))?;
        Ok(())
    }

    pub(super) fn modify(&mut self, node: &Value, frame: &Frame) -> Result<()> {
        if node.get("until").is_some() && node["remove_abilities"] != true {
            return Err(EngineFailure::Unsupported(
                "temporary numeric and cross-turn modification layers".into(),
            ));
        }
        let subjects = self.select(&node["subjects"], frame)?;
        let group = self.group();
        let mut life_changes = Vec::new();
        for id in subjects {
            if let Some(seat) = id.strip_suffix(".leader") {
                let amount = self.number(&node["hp"], frame)?;
                let before = int(&self.player(seat)?.leader["life"]);
                let base = node
                    .get("set_hp")
                    .map_or_else(|| Ok(before), |value| self.number(value, frame))?;
                self.change_life(seat, base.saturating_add(amount))?;
                life_changes.push((seat.to_owned(), before, base.saturating_add(amount)));
                if amount > 0 {
                    self.emit(
                        json!({"kind":"体力増加","target":id,"amount":amount}),
                        &frame.cause,
                        group,
                    );
                }
                continue;
            }
            let prior = self.object(&id)?.state.clone();
            for field in ["power", "hp"] {
                let set_key = format!("set_{field}");
                if let Some(expr) = node.get(&set_key) {
                    let amount = self.number(expr, frame)?;
                    self.object_mut(&id)?.state[field] = json!(amount);
                    if field == "hp" {
                        self.object_mut(&id)?.state["max_hp"] = json!(amount);
                    }
                }
                if let Some(expr) = node.get(field) {
                    let amount = self.number(expr, frame)?;
                    let object = self.object_mut(&id)?;
                    object.state[field] = json!(int(&object.state[field]).saturating_add(amount));
                    if field == "hp" {
                        object.state["max_hp"] =
                            json!(int(&object.state["max_hp"]).saturating_add(amount));
                    }
                }
            }
            if ["power", "hp"]
                .iter()
                .any(|field| int(&self.state.objects[&id].state[*field]) > int(&prior[*field]))
            {
                self.object_mut(&id)?.state["stats_increased_this_turn"] = json!(true);
            }
            if node["remove_abilities"] == true {
                self.object_mut(&id)?.state["silenced"] = json!(true);
                self.object_mut(&id)?.state["keywords"] = json!([]);
            }
            if let Some(keywords) = node["keywords"].as_array() {
                let mut all = list(&self.object(&id)?.state["keywords"]);
                all.extend(keywords.iter().cloned());
                self.object_mut(&id)?.state["keywords"] = json!(all);
            }
            let reference = if node["keywords"].is_array() {
                self.granted_reference(&id, &frame.reference)?
            } else {
                frame.reference.clone()
            };
            self.state.continuous.push(json!({"source":frame.source,"reference":reference,"applies_to":[id],"generation":self.object(&id)?.generation,"effect":node,"until":node.get("until").cloned().unwrap_or_else(||json!("game")),"order":self.state.next_event,"prior_silenced":prior["silenced"],"prior_keywords":prior["keywords"],"duration_controller":self.object(&id)?.controller,"expires_turn":int(&self.state.turn["elapsed_turns"][&self.object(&id)?.controller]).saturating_add(i64::from(self.active()!=self.object(&id)?.controller))}));
        }
        self.life_change_triggers(&life_changes, &frame.cause)?;
        Ok(())
    }

    pub(super) fn draw(&mut self, seat: &str, frame: &Frame) -> Result<()> {
        let deck = self.zone(seat, "deck");
        let Some(top) = deck.first() else {
            self.state.draws_failed.insert(seat.into());
            return Ok(());
        };
        let id = top
            .as_str()
            .ok_or_else(|| EngineFailure::Unsupported("a filler was observed by drawing".into()))?
            .to_owned();
        self.move_objects(from_ref(&id), "hand", None, None, frame)?;
        let group = self.group();
        self.emit(
            json!({"kind":"引く","player":seat,"object":id}),
            &frame.cause,
            group,
        );
        Ok(())
    }

    fn drive(&mut self, frame: &mut Frame) -> Result<()> {
        let Some(id) = self
            .zone(&frame.controller, "deck")
            .first()
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return Err(EngineFailure::Unsupported(
                "empty or filler drive-check".into(),
            ));
        };
        self.move_objects(from_ref(&id), "trigger", None, None, frame)?;
        let card = self.object(&id)?.card.clone();
        let icon = self.state.facts[&card]["trigger_icon"].clone();
        if self.player(&frame.controller)?.construction == "title"
            && matches!(string(&icon), "draw" | "heal" | "critical" | "stand")
        {
            let mut choices = if matches!(string(&icon), "stand" | "critical") {
                self.select(
                    &json!({"side":"self","zone":"field","type":"follower"}),
                    frame,
                )?
                .into_iter()
                .map(|target| json!({"do":"resolve-choice","choice":"execute","select":[target]}))
                .collect::<Vec<_>>()
            } else {
                vec![json!({"do":"resolve-choice","choice":"execute"})]
            };
            choices.push(json!({"do":"resolve-choice","choice":"decline"}));
            self.prompt(
                frame,
                choices,
                json!({"resume":"drive","object":id,"icon":icon}),
            );
        } else {
            self.move_objects(&[id], "deck", None, Some(&json!("bottom")), frame)?;
        }
        Ok(())
    }

    pub(super) fn reveal(&mut self, ids: &[String], to: &str, frame: &Frame, located: bool) {
        let group = self.group();
        for id in ids {
            for seat in ["P1", "P2"] {
                if to == "all" || seat == to {
                    self.learn(seat, id, located);
                }
            }
            self.emit(
                json!({"kind":"公開","object":id,"to":to,"source":frame.source}),
                &frame.cause,
                group,
            );
        }
    }

    pub(super) fn shuffle(&mut self, seat: &str, ids: &[String]) -> Result<()> {
        let mut deck = self.zone(seat, "deck");
        let indexes = deck
            .iter()
            .enumerate()
            .filter(|(_, value)| value.as_str().is_some_and(|id| ids.iter().any(|s| s == id)))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let script = self.state.random["shuffles"]
            .as_array()
            .and_then(|s| s.get(self.state.random_index))
            .cloned();
        if let Some(script) = script {
            if script["player"] != seat {
                return Err(invalid("scripted shuffle belongs to another player"));
            }
            self.state.random_index = self.state.random_index.saturating_add(1);
            let result = list(&script["result"]);
            let mut before = indexes
                .iter()
                .map(|&index| deck[index].to_string())
                .collect::<Vec<_>>();
            before.sort();
            let mut after = result.iter().map(Value::to_string).collect::<Vec<_>>();
            after.sort();
            if before != after {
                return Err(invalid("scripted shuffle is not a permutation"));
            }
            for (index, value) in indexes.iter().zip(result) {
                deck[*index] = value;
            }
        } else {
            for i in (1..indexes.len()).rev() {
                let bound = u64::try_from(i.saturating_add(1)).map_err(invalid)?;
                let j = usize::try_from(
                    self.random_word()
                        .checked_rem(bound)
                        .ok_or_else(|| invalid("zero shuffle bound"))?,
                )
                .map_err(invalid)?;
                if let (Some(&a), Some(&b)) = (indexes.get(i), indexes.get(j)) {
                    deck.swap(a, b);
                }
            }
        }
        self.player_mut(seat)?.zones.insert("deck".into(), deck);
        for knowledge in self.state.knowledge.values_mut() {
            for id in ids {
                knowledge.located.remove(id);
            }
        }
        Ok(())
    }

    fn damage_batch(&mut self, node: &Value, frame: &mut Frame) -> Result<()> {
        let hits = list(&node["hits"]);
        let mut actual = Vec::new();
        for hit in &hits {
            let id = string(&hit["target"]);
            let source = string(&hit["source"]);
            if !id.ends_with(".leader") && self.object(id)?.zone != "field" {
                continue;
            }
            let replacements = self.damage_replacements(hit)?;
            let mut ordered = replacements.clone();
            if replacements.len() > 1 && node["orders"][id].is_null() && int(&hit["amount"]) > 0 {
                let references = replacements
                    .iter()
                    .map(|r| r["reference"].clone())
                    .collect::<Vec<_>>();
                let choices = permutations(&references)
                    .into_iter()
                    .map(|order| json!({"do":"order-replacements","order":order}))
                    .collect();
                self.prompt(
                    frame,
                    choices,
                    json!({"resume":"replacements","task":node,"target":id}),
                );
                let seat = if let Some(seat) = id.strip_suffix(".leader") {
                    seat.to_owned()
                } else {
                    self.object(id)?.controller.clone()
                };
                if let Some(prompt) = self.state.prompt.as_mut() {
                    prompt.by = seat;
                }
                return Ok(());
            }
            if let Some(order) = node["orders"][id].as_array() {
                ordered = order
                    .iter()
                    .filter_map(|reference| {
                        replacements
                            .iter()
                            .find(|r| r["reference"] == *reference)
                            .cloned()
                    })
                    .collect();
            }
            let mut amount = int(&hit["amount"]);
            for replacement in ordered {
                if amount <= 0 {
                    break;
                }
                if replacement["prevent"] == true {
                    amount = 0;
                } else {
                    amount = amount.saturating_add(int(&replacement["amount"]));
                }
            }
            if amount > 0 {
                actual.push(
                    json!({"source":source,"target":id,"amount":amount,"battle":hit["battle"],"attack":hit["attack"]}),
                );
            }
        }
        Self::damage_receipt(&actual, node["bind"].as_str(), frame);
        self.apply_damage(&actual, frame)
    }

    fn apply_damage(&mut self, actual: &[Value], frame: &Frame) -> Result<()> {
        let mut pending = self.drain_triggers(actual, &frame.cause)?;
        let group = self.group();
        let mut damaged = Vec::new();
        let mut life_changes = Vec::new();
        for hit in actual {
            let id = string(&hit["target"]);
            let source = string(&hit["source"]);
            let amount = int(&hit["amount"]);
            if let Some(seat) = id.strip_suffix(".leader") {
                let before = int(&self.player(seat)?.leader["life"]);
                let life = before.saturating_sub(amount);
                self.change_life(seat, life)?;
                life_changes.push((seat.to_owned(), before, life));
                self.bump(&format!("{seat}.leader_damaged"), 1);
            } else {
                let object = self.object_mut(id)?;
                object.state["hp"] = json!(int(&object.state["hp"]).saturating_sub(amount));
                object.state["damage"] = json!(int(&object.state["damage"]).saturating_add(amount));
                if hit["battle"] == true && self.keywords(source)?.contains("bane") {
                    self.object_mut(id)?.state["bane_damaged"] = json!(true);
                }
            }
            damaged.push(self.event_subject(id)?);
            self.emit(
                json!({"kind":"ダメージ","source":source,"target":id,"amount":amount}),
                &frame.cause,
                group,
            );
        }
        pending.extend(self.collect_subject_event(
            "damage",
            &damaged,
            &frame.cause,
            &json!({"effect_damage": !actual.iter().any(|hit|hit["battle"] == true)}),
        )?);
        for hit in actual {
            let source = string(&hit["source"]);
            let subject = self.event_subject(source)?;
            let target = string(&hit["target"]);
            let to_opposing_leader = target.strip_suffix(".leader").is_some_and(|seat| {
                self.object(source)
                    .is_ok_and(|object| object.controller != seat)
            });
            pending.extend(self.collect_subject_event("deal_damage", &[subject], &frame.cause, &json!({"target":target,"amount":hit["amount"],"battle":hit["battle"],"to_opposing_leader":to_opposing_leader}))?);
        }
        self.enqueue(pending);
        self.life_change_triggers(&life_changes, &frame.cause)?;
        Ok(())
    }

    fn damage_receipt(actual: &[Value], bind: Option<&str>, frame: &mut Frame) {
        frame.performed = actual
            .iter()
            .map(|hit| int(&hit["amount"]))
            .fold(0_i64, i64::saturating_add);
        if let Some(name) = bind {
            frame.bindings.insert(
                name.into(),
                actual
                    .iter()
                    .map(|hit| string(&hit["target"]).to_owned())
                    .collect(),
            );
        }
    }

    fn damage_replacements(&self, hit: &Value) -> Result<Vec<Value>> {
        let mut result = Vec::new();
        for source in self.ability_sources() {
            for ability in self.abilities(&source)? {
                let body = &ability["body"];
                if ability["kind"] != "static"
                    || body["op"] != "replace_damage"
                    || !self.ability_zone(&source, &ability)?
                {
                    continue;
                }
                let frame = self.frame_for(&source)?;
                let kind = string(&body["kind"]);
                if kind == "effect" && hit["battle"] == true {
                    continue;
                }
                if matches!(kind, "battle" | "attack") && hit["battle"] != true {
                    continue;
                }
                if let Some(subjects) = body.get("subjects")
                    && !self.matches(string(&hit["target"]), subjects, &frame)?
                {
                    continue;
                }
                if let Some(sources) = body.get("sources")
                    && !self.matches(string(&hit["source"]), sources, &frame)?
                {
                    continue;
                }
                if body.get("uses").is_some() || body.get("until").is_some() {
                    return Err(EngineFailure::Unsupported(
                        "limited-use static damage replacement".into(),
                    ));
                }
                if let Some(condition) = body.get("condition")
                    && !self.truth(condition, &frame)?
                {
                    continue;
                }
                if body.get("set").is_some() {
                    return Err(EngineFailure::Unsupported(
                        "damage replacement setting a value".into(),
                    ));
                }
                result.push(json!({"reference":self.reference(&source,&ability),"amount":self.number(&body["amount"],&frame)?,"prevent":body["prevent"]}));
            }
        }
        Ok(result)
    }
}
