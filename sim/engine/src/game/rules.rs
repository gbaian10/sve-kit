#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use core::mem::take;
use serde_json::{Value, json};

use super::{Frame, Game, Object, Pending, Step, int, list, other, string};
use crate::{EngineFailure, Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    /// Allocates a causal identity for an ordinary match input.
    ///
    /// # Errors
    /// The same transport and semantic errors as `decide`.
    pub fn submit(&mut self, decision: &Value) -> Result<Step> {
        let node = format!("decision-{}", self.state.next_decision);
        let step = self.decide(decision, &node)?;
        self.state.next_decision = self.state.next_decision.saturating_add(1);
        Ok(step)
    }

    /// Applies one complete protocol decision and stops at the next player input.
    ///
    /// # Errors
    /// Returns unsupported semantics or malformed transport data; legal rejections are outcomes.
    pub fn decide(&mut self, decision: &Value, node: &str) -> Result<Step> {
        if !decision.is_object() {
            return Err(invalid("decision must be an object"));
        }
        let before = self.clone();
        self.node = node.into();
        self.emitted.clear();
        let point = self.input_point();
        let mut request = decision.clone();
        for field in ["by", "at"] {
            if request.get(field).is_none() {
                request[field] = point[field].clone();
            }
        }
        match self.decide_inner(&request) {
            Ok(outcome) => Ok(Step {
                outcome,
                events: take(&mut self.emitted),
            }),
            Err(error) => {
                *self = before;
                Err(error)
            }
        }
    }

    fn decide_inner(&mut self, decision: &Value) -> Result<String> {
        let before = self.state.clone();
        let operation = string(&decision["do"]);
        if self.state.game["ended"] == true {
            return Ok("game-end".into());
        }
        let point = self.input_point();
        let by = decision["by"]
            .as_str()
            .unwrap_or_else(|| string(&point["by"]));
        let at = decision["at"]
            .as_str()
            .unwrap_or_else(|| string(&point["at"]));
        if by != string(&point["by"]) || at != string(&point["at"]) {
            return Ok(Self::rejection(operation).into());
        }
        let timing_matches = match at {
            "main" => matches!(
                operation,
                "play" | "activate" | "evolve" | "attack" | "end-phase"
            ),
            "quick" => matches!(operation, "play" | "activate" | "pass"),
            "check-timing" => operation == "choose-pending",
            "resolve" => matches!(
                operation,
                "resolve-choice" | "place-acted" | "order-replacements"
            ),
            "end" => operation == "guard-act",
            _ => false,
        };
        if !timing_matches {
            return Ok(Self::rejection(operation).into());
        }
        let accepted = match operation {
            "play" => self.play(decision)?,
            "activate" => self.activate(decision)?,
            "evolve" => self.evolve_decision(decision)?,
            "attack" => self.attack(decision)?,
            "choose-pending" => return self.choose_pending(decision),
            "resolve-choice" | "place-acted" | "order-replacements" => {
                let matches = self.state.prompt.as_ref().is_some_and(|prompt| {
                    (prompt.resume["resume"] == "declare" && decision["declare"].as_i64().is_some())
                        || prompt
                            .choices
                            .iter()
                            .any(|choice| super::legal::decision_matches(choice, decision))
                });
                if matches {
                    self.resume(decision)?;
                }
                matches
            }
            "end-phase" => {
                self.start_end()?;
                true
            }
            "guard-act" => self.guard_act(decision)?,
            "pass" => {
                self.pass()?;
                true
            }
            _ => {
                return Err(EngineFailure::Unsupported(format!(
                    "decision operation: {operation}"
                )));
            }
        };
        if !accepted {
            self.state = before;
            self.emitted.clear();
            return Ok(Self::rejection(operation).into());
        }
        self.checks()?;
        Ok(if self.state.game["ended"] == true {
            "game-end"
        } else if self.state.prompt.is_some() && operation != "pass" {
            "paused"
        } else {
            "resolved"
        }
        .into())
    }

    fn rejection(operation: &str) -> &'static str {
        match operation {
            "attack" => "cannot-attack",
            "activate" => "cannot-activate",
            "evolve" => "cannot-evolve",
            _ => "cannot-play",
        }
    }

    pub(super) fn input_point(&self) -> Value {
        if self.state.game["ended"] == true {
            return Value::Null;
        }
        if let Some(prompt) = &self.state.prompt {
            return json!({"by":prompt.by,"at":"resolve"});
        }
        if let Some(seat) = self.pending_player() {
            return json!({"by":seat,"at":"check-timing"});
        }
        match string(&self.state.flow["kind"]) {
            "battle" => json!({"by":other(self.active()),"at":"quick"}),
            "end" => {
                if self.state.flow["stage"] == "guard" {
                    json!({"by":self.active(),"at":"end"})
                } else {
                    json!({"by":other(self.active()),"at":"quick"})
                }
            }
            _ => json!({"by":self.active(),"at":"main"}),
        }
    }
    pub(super) fn pending_player(&self) -> Option<&str> {
        if self
            .state
            .pending
            .iter()
            .any(|p| p.controller == self.active())
        {
            Some(self.active())
        } else {
            self.state.pending.first().map(|p| p.controller.as_str())
        }
    }
    pub(super) fn pending_choice(pending: &Pending) -> Value {
        let mut choice = json!({"do":"choose-pending","pending":{"ability":pending.reference}});
        if let Some(id) = &pending.id {
            choice["pending"] = json!(id);
        }
        if pending.id.is_none() && pending.retained {
            choice["pending"]["event"] = pending.event.clone();
        }
        choice
    }

    pub(super) fn start_frame(
        &self,
        source: &str,
        reference: Value,
        decision: &Value,
    ) -> Result<Frame> {
        let mut frame = self.frame_for(source)?;
        frame.reference = reference;
        frame.decision = decision.clone();
        frame.cause = json!({"decision":self.node});
        let mut ids = vec![source.to_owned()];
        for key in ["targets", "costs"] {
            if let Some(entries) = decision[key].as_object() {
                for targets in entries.values() {
                    ids.extend(
                        list(targets)
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned),
                    );
                }
            }
        }
        for id in ids {
            if self.state.objects.contains_key(&id) {
                let state = self.object_attributes(&id)?;
                frame.captured.insert(id, state);
            }
        }
        Ok(frame)
    }

    fn play(&mut self, decision: &Value) -> Result<bool> {
        let Some(frame) = self.prepare_card_play(decision, None)? else {
            return Ok(false);
        };
        self.run_frame(frame)?;
        Ok(true)
    }

    pub(super) fn prepare_card_play(
        &mut self,
        decision: &Value,
        forced_cost: Option<i64>,
    ) -> Result<Option<Frame>> {
        let source = string(&decision["card"]);
        let object = self.object(source)?.clone();
        let controller = string(&decision["by"]);
        if object.controller != controller
            || (forced_cost.is_none() && !self.playable_zone(source)?)
        {
            return Ok(None);
        }
        if decision["at"] == "quick" && !self.keywords(source)?.contains("quick") {
            return Ok(None);
        }
        let face = self.face(source)?.clone();
        let is_spell = string(&face["card_type"]).contains("スペル");
        if !is_spell && self.zone_count(controller, "field") >= 5 {
            return Ok(None);
        }
        let abilities = self.abilities(source)?;
        let spells = abilities
            .iter()
            .filter(|a| a["kind"] == "spell")
            .cloned()
            .collect::<Vec<_>>();
        let mut frame = self.start_frame(source, Value::Null, decision)?;
        for ability in &spells {
            if !self.prepare_additional(ability, &mut frame)? {
                return Ok(None);
            }
            self.freeze(ability, "play-start", &mut frame)?;
            if !self.valid_parameters(ability, &frame)? {
                return Ok(None);
            }
        }
        let cost = forced_cost.map_or_else(|| self.play_cost_context(source, &frame), Ok)?;
        if int(&self.player(controller)?.pp["current"]) < cost {
            return Ok(None);
        }
        self.player_mut(controller)?.pp["current"] =
            json!(int(&self.player(controller)?.pp["current"]).saturating_sub(cost));
        let mandatory = spells
            .iter()
            .flat_map(|code| Self::payment_nodes(code, &frame))
            .collect::<Vec<_>>();
        if !self.can_pay(&mandatory, &frame)? {
            return Ok(None);
        }
        for code in &spells {
            self.pay_costs(code, &mut frame)?;
        }
        for code in &spells {
            self.notify_ability_start(&frame, code)?;
        }
        self.move_objects(&[source.into()], "resolution", None, None, &frame)?;
        self.object_mut(source)?.state["entered_from"] = json!(object.zone);
        self.bump(&format!("{controller}.cards_played"), 1);
        let group = self.group();
        let cause = self.emit(
            json!({"kind":"プレイ","object":source}),
            &frame.cause,
            group,
        );
        frame.cause = json!({"event":cause});
        let resolve_group = self.group();
        self.emit(
            json!({"kind":"解決","object":source}),
            &frame.cause,
            resolve_group,
        );
        if is_spell {
            for ability in &spells {
                self.freeze(ability, "resolution-start", &mut frame)?;
            }
            frame.todo = spells
                .iter()
                .flat_map(|code| {
                    [
                        json!({"op":"_reference","reference":self.reference(source,code)}),
                        code["body"].clone(),
                    ]
                })
                .collect();
        } else {
            frame
                .todo
                .push(json!({"op":"move","subjects":"self","to":"field"}));
        }
        frame.todo.push(json!({"op":"_finish_card"}));
        Ok(Some(frame))
    }

    fn activate(&mut self, decision: &Value) -> Result<bool> {
        let reference = &decision["ability"];
        let source = string(&reference["source"]);
        let code = self.ability(source, reference)?;
        if (matches!(string(&code["kind"]), "meal" | "ride") || code["advance"] == true) {
            return self.resource_activation(decision, &code);
        }
        if code["kind"] != "activated"
            || !self.ability_zone(source, &code)?
            || !self.can_use(source, &code)?
        {
            return Ok(false);
        }
        if self.object(source)?.controller != string(&decision["by"]) {
            return Ok(false);
        }
        if decision["at"] == "quick" && code["quick"] != true {
            return Ok(false);
        }
        let mut frame = self.start_frame(source, reference.clone(), decision)?;
        self.freeze(&code, "play-start", &mut frame)?;
        if !self.prepare_additional(&code, &mut frame)? {
            return Ok(false);
        }
        if !self.valid_parameters(&code, &frame)? || !self.can_pay(&list(&code["costs"]), &frame)? {
            return Ok(false);
        }
        self.mark_use(source, &code)?;
        self.pay_costs(&code, &mut frame)?;
        self.begin_ability(&mut frame, &code)?;
        Ok(true)
    }

    fn choose_pending(&mut self, decision: &Value) -> Result<String> {
        let choice = &decision["pending"];
        let mut matches = self.state.pending.iter().enumerate().filter(|(_, p)| {
            if choice.is_string() {
                return p.id.as_deref() == choice.as_str();
            }
            super::legal::reference_matches(&choice["ability"], &p.reference)
                && (choice["event"].is_null() || choice["event"] == p.event)
        });
        let (index, first) = matches
            .next()
            .ok_or_else(|| invalid(format!("pending ability not present: {choice}")))?;
        if matches.any(|(_, pending)| {
            pending.reference != first.reference
                || pending.event != first.event
                || pending.code != first.code
                || pending.controller != first.controller
                || pending.context != first.context
        }) {
            return Err(invalid(format!("pending ability is ambiguous: {choice}")));
        }
        let pending = self.state.pending.remove(index);
        if pending.controller != string(&decision["by"]) {
            return Err(invalid("wrong pending controller"));
        }
        if decision["costs"] == "decline" && list(&pending.code["costs"]).is_empty() {
            return Err(invalid("a cost-free pending ability cannot be declined"));
        }
        let mut frame = self.start_frame(&pending.source, pending.reference.clone(), decision)?;
        if let Some(context) = &pending.context {
            frame.bindings.clone_from(&context.bindings);
            for (id, attributes) in &context.captured {
                if id == &context.source {
                    frame.captured.insert(id.clone(), attributes.clone());
                    continue;
                }
                frame
                    .captured
                    .entry(id.clone())
                    .or_insert_with(|| attributes.clone());
            }
            frame.event.clone_from(&context.event);
            frame.values.clone_from(&context.values);
            frame.controller.clone_from(&context.controller);
        }
        self.freeze(&pending.code, "play-start", &mut frame)?;
        if !self.prepare_additional(&pending.code, &mut frame)? {
            return Ok("cannot-play".into());
        }
        if decision["costs"] == "decline"
            || !self.valid_parameters(&pending.code, &frame)?
            || !self.can_pay(&Self::payment_nodes(&pending.code, &frame), &frame)?
        {
            let group = self.group();
            self.emit(
                json!({"kind":"待機取消","ability":pending.reference,"event":pending.event}),
                &json!({"decision":self.node}),
                group,
            );
            self.checks()?;
            return Ok("pending-cancelled".into());
        }
        self.pay_costs(&pending.code, &mut frame)?;
        self.begin_ability(&mut frame, &pending.code)?;
        self.checks()?;
        Ok(if self.state.game["ended"] == true {
            "game-end"
        } else if self.state.prompt.is_some() {
            "paused"
        } else {
            "resolved"
        }
        .into())
    }

    fn notify_ability_start(&mut self, frame: &Frame, code: &Value) -> Result<()> {
        let mut targets = Vec::new();
        if let Some(selections) = frame.decision["targets"].as_object() {
            for selected in selections.values() {
                for id in list(selected) {
                    if let Some(object) = self.state.objects.get(string(&id))
                        && !targets.iter().any(|target: &Object| target.id == object.id)
                    {
                        targets.push(object.clone());
                    }
                }
            }
        }
        let pending = self.collect_triggers("targeted", &targets, &frame.cause)?;
        self.enqueue(pending);
        if code["ub"] == true {
            self.bump(&format!("{}.ub_activated", frame.controller), 1);
            let affected = [self.object(&frame.source)?.clone()];
            let pending_ub = self.collect_triggers("ub_activated", &affected, &frame.cause)?;
            self.enqueue(pending_ub);
        }
        Ok(())
    }

    pub(super) fn begin_ability(&mut self, frame: &mut Frame, code: &Value) -> Result<()> {
        self.prepare_ability(frame, code)?;
        self.run_frame(frame.clone())
    }

    pub(super) fn prepare_ability(&mut self, frame: &mut Frame, code: &Value) -> Result<()> {
        self.notify_ability_start(frame, code)?;
        self.freeze(code, "resolution-start", frame)?;
        let group = self.group();
        let id = self.emit(
            json!({"kind":"プレイ","ability":frame.reference}),
            &frame.cause,
            group,
        );
        frame.cause = json!({"event":id});
        frame.todo = vec![code["body"].clone(), json!({"op":"_finish_ability"})];
        Ok(())
    }

    pub(super) fn pay_costs(&mut self, code: &Value, frame: &mut Frame) -> Result<()> {
        let costs = Self::payment_nodes(code, frame);
        if costs.is_empty() {
            return Ok(());
        }
        let start = self.emitted.len();
        let group = self.group();
        let mut cost_frame = frame.clone();
        cost_frame.todo.clone_from(&costs);
        cost_frame.values.insert("paying_cost".into(), json!(true));
        self.run_frame(cost_frame)?;
        for event in self.emitted.iter_mut().skip(start) {
            event["group"] = json!(group);
        }
        let zero = costs.iter().all(|cost| match string(&cost["op"]) {
            "pp" => self.number(&cost["amount"], frame).unwrap_or(1) == 0,
            _ => self
                .select(&cost["subjects"], frame)
                .is_ok_and(|ids| ids.is_empty()),
        });
        let replaced = self
            .emitted
            .iter()
            .skip(start)
            .any(|event| event["kind"] == "取代");
        let paid_group = self.group();
        let mut event = json!({"kind":"費用成立","ability":frame.reference});
        if zero {
            event["zero"] = json!(true);
        }
        if replaced {
            event["replaced"] = json!(true);
        }
        self.emit(event, &frame.cause, paid_group);
        frame.paid = true;
        Ok(())
    }

    fn evolve_decision(&mut self, decision: &Value) -> Result<bool> {
        self.evolve_action(decision)
    }

    fn attack(&mut self, decision: &Value) -> Result<bool> {
        let id = string(&decision["attacker"]);
        let target = string(&decision["target"]);
        if !self.can_attack(id, target)? {
            return Ok(false);
        }
        self.object_mut(id)?.state["acted"] = json!(true);
        self.bump(&format!("{}.attacks", self.active()), 1);
        self.bump(&format!("{id}.attacks"), 1);
        for trait_name in list(&self.face(id)?["traits"]) {
            self.bump(
                &format!("{}.trait_attacks.{}", self.active(), string(&trait_name)),
                1,
            );
        }
        let cause = json!({"decision":self.node});
        let group = self.group();
        let event = self.emit(
            json!({"kind":"攻撃","attacker":id,"target":target}),
            &cause,
            group,
        );
        self.state.flow = json!({"kind":"battle","attacker":id,"target":target,"decision":decision,"cause":{"event":event}});
        let pending = self.collect_triggers(
            "attack",
            &[self.object(id)?.clone()],
            &json!({"event":event}),
        )?;
        self.enqueue(pending);
        Ok(true)
    }

    fn pass(&mut self) -> Result<()> {
        match string(&self.state.flow["kind"]) {
            "battle" => {
                let flow = self.state.flow.clone();
                let source = string(&flow["attacker"]);
                let target = string(&flow["target"]);
                self.state.flow = json!({"kind":"main"});
                if self.object(source)?.zone != "field"
                    || (!target.ends_with(".leader") && self.object(target)?.zone != "field")
                {
                    return Ok(());
                }
                let mut frame = self.start_frame(source, Value::Null, &flow["decision"])?;
                frame.cause = flow["cause"].clone();
                let mut hits = vec![
                    json!({"source":source,"target":target,"amount":self.object(source)?.state["power"],"battle":true}),
                ];
                if !target.ends_with(".leader") {
                    hits.push(json!({"source":target,"target":source,"amount":self.object(target)?.state["power"],"battle":true}));
                    if self.keywords(source)?.contains("bane") {
                        self.object_mut(target)?.state["bane_damaged"] = json!(true);
                    }
                    if self.keywords(target)?.contains("bane") {
                        self.object_mut(source)?.state["bane_damaged"] = json!(true);
                    }
                }
                frame.todo = vec![json!({"op":"_damage","hits":hits,"orders":{}})];
                self.run_frame(frame)?;
            }
            "end" => self.next_turn()?,
            _ => return Err(invalid("pass outside Quick")),
        }
        Ok(())
    }

    fn start_end(&mut self) -> Result<()> {
        self.state.turn["phase"] = json!("end");
        self.state.flow = json!({"kind":"end","stage":"triggers"});
        let pending = self.collect_triggers("end", &[], &json!({"decision":self.node}))?;
        self.enqueue(pending);
        self.enqueue_delayed_end()?;
        Ok(())
    }
    fn guard_act(&mut self, decision: &Value) -> Result<bool> {
        for id in list(&decision["select"]) {
            let id = string(&id);
            if self.object(id)?.controller != self.active() || !self.keywords(id)?.contains("guard")
            {
                return Ok(false);
            }
            self.object_mut(id)?.state["acted"] = json!(true);
            let group = self.group();
            self.emit(
                json!({"kind":"アクト","object":id}),
                &json!({"decision":self.node}),
                group,
            );
        }
        self.state.flow["stage"] = json!("quick");
        Ok(true)
    }
    fn next_turn(&mut self) -> Result<()> {
        self.expire_silence()?;
        let mut extra_turns = list(&self.state.turn["extra_turns"]);
        let seat = if extra_turns.is_empty() {
            other(self.active()).to_owned()
        } else {
            string(&extra_turns.remove(0)).to_owned()
        };
        self.state.turn["extra_turns"] = json!(extra_turns);
        self.state.turn["active"] = json!(seat);
        self.state.turn["phase"] = json!("main");
        self.state.flow = json!({"kind":"main"});
        self.state.turn["elapsed_turns"][&seat] =
            json!(int(&self.state.turn["elapsed_turns"][&seat]).saturating_add(1));
        for value in self.state.counters.values_mut() {
            *value = 0;
        }
        self.state.used.clear();
        for object in self.state.objects.values_mut() {
            object.state["stats_increased_this_turn"] = json!(false);
        }
        let prevent_gain = self.restricted(&format!("{seat}.leader"), "normal_max_pp_gain")?;
        let prevent_draw = self.restricted(&format!("{seat}.leader"), "normal_draw")?;
        self.state.continuous.retain(|entry| {
            !(entry["during"] == "next-opponent-start"
                && list(&entry["applies_to"]).contains(&json!(format!("{seat}.leader"))))
        });
        let player = self.player_mut(&seat)?;
        let max = int(&player.pp["max"])
            .saturating_add(i64::from(!prevent_gain))
            .min(10);
        player.pp = json!({"current":max,"max":max});
        for id in self.zone_ids(&seat, "field") {
            if !self.restricted(&id, "normal_stand")? {
                self.object_mut(&id)?.state["acted"] = json!(false);
            }
            self.object_mut(&id)?.state["entered_this_turn"] = json!(false);
        }
        let frame = Frame {
            controller: seat.clone(),
            cause: json!({"rule":"7.2"}),
            ..Frame::default()
        };
        if !prevent_draw {
            self.draw(&seat, &frame)?;
        }
        let pending = self.collect_triggers("main_start", &[], &json!({"rule":"7.3"}))?;
        self.enqueue(pending);
        Ok(())
    }

    pub(super) fn checks(&mut self) -> Result<()> {
        if self.state.frame.is_some() {
            return Ok(());
        }
        let losers = ["P1", "P2"]
            .into_iter()
            .filter(|seat| {
                self.state
                    .players
                    .get(*seat)
                    .is_some_and(|p| int(&p.leader["life"]) <= 0)
                    || self.state.draws_failed.contains(*seat)
            })
            .collect::<Vec<_>>();
        if !losers.is_empty() {
            let rule = if losers.len() == 2 {
                "1.2.2"
            } else if losers
                .first()
                .is_some_and(|s| self.state.draws_failed.contains(*s))
            {
                "11.2.2"
            } else {
                "11.2.1"
            };
            self.state.game = json!({"ended":true,"winner":if losers.len()==2 {None} else {losers.first().map(|s|other(s))},"by":rule});
            let group = self.group();
            for seat in losers {
                self.emit(
                    json!({"kind":"敗北","player":seat,"by":format!("rule-{rule}")}),
                    &json!({"rule":rule}),
                    group,
                );
            }
            return Ok(());
        }
        let dead =
            self.field_ids()
                .into_iter()
                .filter(|id| {
                    self.state.objects.get(id).is_some_and(|o| {
                        int(&o.state["hp"]) <= 0 || o.state["bane_damaged"] == true
                    }) && self
                        .object_type(id)
                        .is_ok_and(|typ| typ.contains("フォロワー"))
                })
                .collect::<Vec<_>>();
        if !dead.is_empty() {
            self.destroy(&dead, None)?;
            return self.checks();
        }
        if self.state.pending.is_empty()
            && self.state.flow["kind"] == "end"
            && self.state.flow["stage"] == "triggers"
        {
            let guards = self.zone_ids(self.active(), "field").iter().any(|id| {
                self.object(id).is_ok_and(|o| o.state["acted"] != true)
                    && self.keywords(id).is_ok_and(|k| k.contains("guard"))
            });
            self.state.flow["stage"] = json!(if guards { "guard" } else { "quick" });
        }
        Ok(())
    }

    pub(super) fn collect_triggers(
        &mut self,
        event: &str,
        affected: &[Object],
        cause: &Value,
    ) -> Result<Vec<Pending>> {
        self.collect_event(event, affected, cause, &Value::Null)
    }

    pub(super) fn collect_event(
        &mut self,
        event: &str,
        affected: &[Object],
        cause: &Value,
        metadata: &Value,
    ) -> Result<Vec<Pending>> {
        let mut result = Vec::new();
        for source in self.ability_sources() {
            for code in self.abilities(&source)? {
                if code["kind"] != "trigger"
                    || code["event"] != event
                    || !self.ability_zone(&source, &code)?
                {
                    continue;
                }
                let mut frame = self.frame_for(&source)?;
                let phase_event = matches!(event, "end" | "main_start" | "drive_trigger");
                if phase_event
                    && !self
                        .seats(string(&code["side"]), &frame)
                        .iter()
                        .any(|seat| seat == self.active())
                {
                    continue;
                }
                frame
                    .captured
                    .insert(source.clone(), self.object_attributes(&source)?);
                let candidates = if phase_event {
                    vec![None]
                } else {
                    affected.iter().map(Some).collect()
                };
                for object in candidates {
                    if let Some(object) = object
                        && let Some(selector) = code.get("subject")
                        && !self.matches(&object.id, selector, &frame)?
                    {
                        continue;
                    }
                    frame.event = if metadata.is_object() {
                        metadata.clone()
                    } else {
                        json!({})
                    };
                    if let Some(object) = object {
                        frame.event["subject"] = self.object_attributes(&object.id)?;
                        frame.event["subject_id"] = json!(object.id);
                    }
                    if event == "attack" {
                        frame.event["target"] = self.state.flow["target"].clone();
                        frame.event["target_is_follower"] =
                            json!(!string(&self.state.flow["target"]).ends_with(".leader"));
                    }
                    if let Some(condition) = code.get("trigger_if")
                        && !self.truth(condition, &frame)?
                    {
                        continue;
                    }
                    if !self.can_use(&source, &code)? {
                        continue;
                    }
                    if code["limit_at"] == "trigger" {
                        self.mark_use(&source, &code)?;
                    }
                    if matches!(event, "enter" | "evolve")
                        && self.trigger_suppressed(&frame.controller, event)?
                    {
                        continue;
                    }
                    let event_key = match event {
                        "enter" => "entered_field",
                        "leave" | "field_to_cemetery" => "left_field",
                        "damage" => "damaged",
                        "evolve" => "evolved",
                        "attack" => "attacked",
                        _ => "",
                    };
                    let detail = object.map_or(Value::Null, |object| json!({event_key:object.id}));
                    let copies = self.trigger_copies(event, &frame.controller)?;
                    for _ in 0..copies {
                        result.push(Pending {
                            controller: frame.controller.clone(),
                            reference: self.reference(&source, &code),
                            event: detail.clone(),
                            code: code.clone(),
                            source: source.clone(),
                            cause: cause.clone(),
                            retained: false,
                            id: None,
                            context: Some(frame.clone()),
                        });
                    }
                }
            }
        }
        Ok(result)
    }

    fn trigger_suppressed(&self, controller: &str, event: &str) -> Result<bool> {
        for source in self.field_ids() {
            for code in self.printed_abilities(&source)? {
                let body = &code["body"];
                if body["op"] == "restrict"
                    && body["action"] == "trigger"
                    && list(&body["events"]).contains(&json!(if event == "enter" {
                        "fanfare"
                    } else {
                        "on_evolve"
                    }))
                {
                    let frame = self.frame_for(&source)?;
                    if self
                        .select(&body["subjects"], &frame)?
                        .contains(&format!("{controller}.leader"))
                    {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
    fn trigger_copies(&self, event: &str, controller: &str) -> Result<i64> {
        let mut count = 1_i64;
        for source in self.field_ids() {
            for code in self.printed_abilities(&source)? {
                let body = &code["body"];
                if body["op"] != "repeat_triggers" || body["event"] != event {
                    continue;
                }
                let frame = self.frame_for(&source)?;
                if self
                    .seats(string(&body["side"]), &frame)
                    .iter()
                    .any(|seat| seat == controller)
                {
                    count = count.saturating_add(self.number(&body["additional"], &frame)?);
                }
            }
        }
        Ok(count.max(0))
    }
    pub(super) fn enqueue(&mut self, batch: Vec<Pending>) {
        let group = self.group();
        for pending in batch {
            let mut event = json!({"kind":"待機","ability":pending.reference});
            if !pending.event.is_null() {
                event["event"] = pending.event.clone();
            }
            self.emit(event, &pending.cause, group);
            self.push_pending(pending);
        }
    }
    pub(super) fn push_pending(&mut self, mut pending: Pending) {
        if pending.id.is_none() {
            for other in self.state.pending.iter_mut().filter(|other| {
                other.id.is_none()
                    && other.controller == pending.controller
                    && other.reference == pending.reference
                    && other.event != pending.event
            }) {
                // An instance keeps its discriminator after its siblings resolve.
                other.retained = true;
                pending.retained = true;
            }
        }
        self.state.pending.push(pending);
    }

    pub(super) fn restricted(&self, id: &str, action: &str) -> Result<bool> {
        for entry in &self.state.continuous {
            if entry["effect"]["op"] == "restrict"
                && entry["effect"]["action"] == action
                && self.continuous_applies(entry, id)
            {
                return Ok(true);
            }
        }
        for source in self.field_ids() {
            for code in self.abilities(&source)? {
                let body = &code["body"];
                if code["kind"] != "static" || body["op"] != "restrict" || body["action"] != action
                {
                    continue;
                }
                let frame = self.frame_for(&source)?;
                if let Some(condition) = body.get("condition")
                    && !self.truth(condition, &frame)?
                {
                    continue;
                }
                if self.matches(id, &body["subjects"], &frame)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(crate) fn manual(&mut self, operation: &Value, node: &str) -> Result<Step> {
        self.node = node.into();
        self.emitted.clear();
        match string(&operation["do"]) {
            "attack" => {
                let id = string(&operation["attacker"]);
                self.object_mut(id)?.state["acted"] = json!(true);
                let group = self.group();
                self.emit(
                    json!({"kind":"攻撃","attacker":id,"target":operation["target"]}),
                    &json!({"decision":node}),
                    group,
                );
            }
            "set" => {
                let path = string(&operation["path"]);
                let parts = path.split('.').collect::<Vec<_>>();
                match parts.as_slice() {
                    [seat, "leader", "life"] => {
                        self.player_mut(seat)?.leader["life"] = operation["value"].clone();
                    }
                    _ => {
                        return Err(EngineFailure::Unsupported(format!(
                            "manual property: {path}"
                        )));
                    }
                }
            }
            _ => {
                return Err(EngineFailure::Unsupported(
                    "manual atom not implemented".into(),
                ));
            }
        }
        Ok(Step {
            outcome: "resolved".into(),
            events: take(&mut self.emitted),
        })
    }
}
