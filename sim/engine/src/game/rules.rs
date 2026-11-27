#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use crate::ir::Phase;
use crate::ir::{ELEMENTAL_TRAITS, trait_matches};
use core::mem::take;
use serde_json::{Value, json};

use super::{Frame, Game, Step, int, list, other, string};
use crate::ir::SnapshotId;
use crate::{EngineFailure, Result, invalid};

impl Game {
    pub(in crate::game) fn apply_rule_decision(
        &mut self,
        decision: &Value,
        node: &str,
    ) -> Result<Step> {
        if !decision.is_object() {
            return Err(invalid("decision must be an object"));
        }
        let before = self.clone();
        let layered = self.internal_numeric_active();
        self.node = node.into();
        self.emitted.clear();
        let point = self.input_point();
        let mut request = decision.clone();
        for field in ["by", "at"] {
            if request.get(field).is_none() {
                request[field] = point[field].clone();
            }
        }
        match self.decide_inner(&request).and_then(|outcome| {
            // Grants or new objects could add triggers whose waits typed layers cannot order yet;
            // the last layer may expire within this input after such a wait already happened.
            if layered || self.internal_numeric_active() {
                self.check_internal_fixture()?;
            }
            Ok(outcome)
        }) {
            Ok(outcome) => Ok(Step {
                outcome,
                events: take(&mut self.emitted),
            }),
            Err(error) => {
                let high = self.state.ir_epoch;
                *self = before;
                self.state.ir_epoch = self.state.ir_epoch.max(high);
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
            "main" => matches!(operation, "evolve" | "attack" | "end-phase"),
            "quick" => operation == "pass",
            "pregame" => matches!(
                operation,
                "choose-start-amulet" | "choose-first" | "mulligan"
            ),
            "end" => matches!(operation, "guard-act" | "end-discard"),
            _ => false,
        };
        if !timing_matches {
            return Ok(Self::rejection(operation).into());
        }
        let accepted = match operation {
            "choose-start-amulet" | "choose-first" | "mulligan" => {
                self.opening_decision(decision)?
            }
            "evolve" => self.evolve_decision(decision)?,
            "attack" => self.attack(decision)?,
            "end-phase" => {
                if self.attack_required()? {
                    false
                } else {
                    self.start_end()?;
                    true
                }
            }
            "guard-act" => self.guard_act(decision)?,
            "end-discard" => self.end_discard(decision)?,
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
        // Battle checks require all simultaneous hits to finish ordering and commit.
        if self.typed.is_none() {
            self.checks()?;
        }
        Ok(if self.state.game["ended"] == true {
            "game-end"
        } else {
            "resolved"
        }
        .into())
    }

    fn rejection(operation: &str) -> &'static str {
        match operation {
            "attack" => "cannot-attack",
            "evolve" => "cannot-evolve",
            _ => "cannot-play",
        }
    }

    pub(super) fn input_point(&self) -> Value {
        if self.state.game["ended"] == true {
            return Value::Null;
        }
        if self.state.flow["kind"] == "pregame" {
            return self.opening_point();
        }
        match string(&self.state.flow["kind"]) {
            "battle" => json!({"by":other(self.active()),"at":"quick"}),
            "end" => {
                if matches!(string(&self.state.flow["stage"]), "guard" | "discard") {
                    json!({"by":self.active(),"at":"end"})
                } else {
                    json!({"by":other(self.active()),"at":"quick"})
                }
            }
            _ => json!({"by":self.active(),"at":"main"}),
        }
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
        Ok(frame)
    }

    fn evolve_decision(&mut self, decision: &Value) -> Result<bool> {
        let source = self.object(string(&decision["source"]))?;
        self.require_internal_abilities(&source.id, "evolution")?;
        self.internal_evolve_action(decision)
    }

    fn attack(&mut self, decision: &Value) -> Result<bool> {
        let id = string(&decision["attacker"]);
        let target = string(&decision["target"]);
        self.apply_attack(id, target, Some(decision))
    }

    pub(in crate::game) fn apply_attack(
        &mut self,
        id: &str,
        target: &str,
        original: Option<&Value>,
    ) -> Result<bool> {
        // Preserve the attack's declaration in the referee flow.
        let request =
            json!({"do":"attack","attacker":id,"target":target,"by":self.active(),"at":"main"});
        let decision = original.unwrap_or(&request);
        if !self.can_attack(id, target)? {
            return Ok(false);
        }
        let before = SnapshotId(self.state.ir_epoch);
        let cause = json!({"decision":self.node});
        self.change_posture(&[id.into()], true, &cause)?;
        self.bump(&format!("{}.attacks", self.active()), 1);
        self.bump(&format!("{id}.attacks"), 1);
        let attacks = int(&self.object(id)?.state["attacks_this_turn"]);
        self.object_mut(id)?.state["attacks_this_turn"] = json!(attacks.saturating_add(1));
        let traits = self.card_traits(id)?;
        let elemental_references = ELEMENTAL_TRAITS.into_iter().filter(|attribute| {
            !traits.contains(*attribute)
                && traits.iter().any(|actual| trait_matches(actual, attribute))
        });
        for trait_name in traits
            .iter()
            .map(String::as_str)
            .chain(elemental_references)
        {
            self.bump(&format!("{}.trait_attacks.{trait_name}", self.active()), 1);
        }
        let group = self.group();
        let event = self.emit(
            json!({"kind":"攻撃","attacker":id,"target":target}),
            &cause,
            group,
        );
        let actor = self.active().to_owned();
        self.player_step_actor(&event, &actor);
        self.state.flow = json!({"kind":"battle","attacker":id,"attacker_generation":self.object(id)?.generation,"target":target,"target_generation":if target.ends_with(".leader") { None } else { Some(self.object(target)?.generation) },"decision":decision,"cause":{"event":event}});
        self.require_ability_sources("event attack subscriptions")?;
        self.group();
        self.internal_attack_event(id, target, before)?;
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
                    || flow["attacker_generation"]
                        .as_u64()
                        .is_some_and(|generation| {
                            self.object(source)
                                .is_ok_and(|o| o.generation != generation)
                        })
                    || (!target.ends_with(".leader")
                        && (self.object(target)?.zone != "field"
                            || flow["target_generation"]
                                .as_u64()
                                .is_some_and(|generation| {
                                    self.object(target)
                                        .is_ok_and(|o| o.generation != generation)
                                })))
                {
                    return Ok(());
                }
                let mut frame = self.start_frame(source, Value::Null, &flow["decision"])?;
                frame.cause = flow["cause"].clone();
                let mut hits = vec![
                    json!({"source":source,"source_generation":self.object(source)?.generation,"target":target,"amount":self.internal_combat_damage(source)?,"battle":true,"attack":true}),
                ];
                if !target.ends_with(".leader") {
                    hits.push(json!({"source":target,"source_generation":self.object(target)?.generation,"target":source,"amount":self.internal_combat_damage(target)?,"battle":true}));
                    if self.keywords(source)?.contains("bane") {
                        self.object_mut(target)?.state["bane_damaged"] = json!(true);
                    }
                    if self.keywords(target)?.contains("bane") {
                        self.object_mut(source)?.state["bane_damaged"] = json!(true);
                    }
                }
                self.internal_battle_damage(frame, hits)?;
            }
            "end" => self.state.flow["stage"] = json!("discard"),
            _ => return Err(invalid("pass outside Quick")),
        }
        Ok(())
    }

    /// Opening and turn start share this entry so Main Begin fires exactly once per turn.
    pub(super) fn start_main(&mut self) -> Result<()> {
        self.state.turn["phase"] = json!("main");
        self.record_player_phase();
        self.state.flow = json!({"kind":"main"});
        self.internal_phase_event(Phase::Main, SnapshotId(self.state.ir_epoch))?;
        self.require_ability_sources("event main_start subscriptions")?;
        self.group();
        Ok(())
    }

    pub(in crate::game) fn start_end(&mut self) -> Result<()> {
        let before = SnapshotId(self.state.ir_epoch);
        self.state.turn["phase"] = json!("end");
        self.record_player_phase();
        self.state.flow = json!({"kind":"end","stage":"triggers"});
        self.require_ability_sources("event end subscriptions")?;
        // Preserve the coordinates of the end-phase and delayed-event batches in the referee trace.
        self.group();
        self.group();
        self.internal_phase_event(Phase::End, before)?;
        Ok(())
    }
    fn guard_act(&mut self, decision: &Value) -> Result<bool> {
        if self.state.flow["stage"] != "guard"
            || !self
                .end_choices(self.active())?
                .iter()
                .any(|choice| super::legal::unordered_selection_matches(choice, decision))
        {
            return Ok(false);
        }
        let selected = list(&decision["select"])
            .iter()
            .map(|id| string(id).to_owned())
            .collect::<Vec<_>>();
        for id in &selected {
            if self.object(id)?.controller != self.active() || !self.keywords(id)?.contains("guard")
            {
                return Ok(false);
            }
        }
        self.apply_end_guard(&selected)?;
        Ok(true)
    }

    pub(in crate::game) fn apply_end_guard(&mut self, selected: &[String]) -> Result<()> {
        self.change_posture(selected, true, &json!({"decision":self.node}))?;
        self.state.flow["stage"] = json!("quick");
        Ok(())
    }

    fn end_discard(&mut self, decision: &Value) -> Result<bool> {
        if self.state.flow["stage"] != "discard"
            || !self
                .end_choices(self.active())?
                .iter()
                .any(|choice| super::legal::unordered_selection_matches(choice, decision))
        {
            return Ok(false);
        }
        let selected = list(&decision["select"])
            .iter()
            .map(|id| string(id).to_owned())
            .collect::<Vec<_>>();
        self.apply_end_discard(&selected)?;
        Ok(true)
    }

    pub(in crate::game) fn apply_end_discard(&mut self, selected: &[String]) -> Result<()> {
        let frame = Frame {
            controller: self.active().into(),
            cause: json!({"rule":"7.4.7"}),
            ..Frame::default()
        };
        let pending = self.internal_prepare_rule_discard(selected, &frame)?;
        self.discard(selected, &frame)?;
        self.internal_commit_rule_discard(pending)?;
        Ok(())
    }

    pub(in crate::game) fn hand_excess(&self, seat: &str) -> Result<u64> {
        if self.player(seat)?.unlimited_hand {
            return Ok(0);
        }
        u64::try_from(self.zone_count(seat, "hand").saturating_sub(7).max(0)).map_err(invalid)
    }

    fn next_turn(&mut self) -> Result<()> {
        let turn = self.state.schedule.close()?;
        self.internal_numeric_end_turn(turn)?;
        self.internal_keywords_end_turn(turn);
        self.internal_suppressions_end_turn(turn);
        self.internal_drive_end_turn(turn);
        self.internal_attack_restrictions_end_turn(turn);
        self.internal_ability_restrictions_end_turn(turn);
        self.internal_delayed_end_turn(turn);
        self.internal_player_play_end_turn(turn);
        self.internal_preventions_end_turn(turn);
        self.internal_phase_restrictions_end_turn();
        let seat = self.next_scheduled_player()?;
        self.state.turn["active"] = json!(seat);
        self.state.schedule.begin()?;
        self.internal_numeric_begin_turn()?;
        self.internal_keywords_begin_turn()?;
        self.internal_suppressions_begin_turn()?;
        self.internal_preventions_begin_turn()?;
        self.internal_player_cost_begin_turn()?;
        self.internal_attack_restrictions_begin_turn()?;
        self.state.turn["phase"] = json!("start");
        self.state.flow = json!({"kind":"main"});
        self.state.turn["elapsed_turns"][&seat] =
            json!(int(&self.state.turn["elapsed_turns"][&seat]).saturating_add(1));
        self.record_player_phase();
        self.internal_reset_turn_counters_and_facts();
        self.internal_reset_object_turn_marks();
        let prevent_gain = self.restricted(&format!("{seat}.leader"), "normal_max_pp_gain")?;
        let prevent_draw = self.restricted(&format!("{seat}.leader"), "normal_draw")?;
        let player = self.player_mut(&seat)?;
        let (before_max, before) = (int(&player.pp["max"]), int(&player.pp["current"]));
        let max = before_max.saturating_add(i64::from(!prevent_gain)).min(10);
        player.pp = json!({"current":max,"max":max});
        self.record_player_resource(
            &seat,
            "maximum_pp",
            before_max,
            max,
            &json!({"rule":"7.2.1"}),
        );
        self.record_player_resource(&seat, "pp", before, max, &json!({"rule":"7.2.2"}));
        let mut standing = Vec::new();
        for id in self.zone_ids(&seat, "field") {
            if !self.restricted(&id, "normal_stand")? && !self.internal_rule_stand_blocked(&id)? {
                standing.push(id.clone());
            }
        }
        // "このターン中に場に出た" covers both fields, so every new turn clears both.
        for id in [self.zone_ids("P1", "field"), self.zone_ids("P2", "field")].concat() {
            self.object_mut(&id)?.state["entered_this_turn"] = json!(false);
        }
        self.change_posture(&standing, false, &json!({"rule":"7.2.3"}))?;
        self.internal_finish_rule_stand()?;
        let frame = Frame {
            controller: seat.clone(),
            cause: json!({"rule":"7.2.4"}),
            ..Frame::default()
        };
        if !prevent_draw {
            drop(self.draw(&seat, &frame, super::player::events::MovementMethod::Draw)?);
        }
        // Start checks can leave typed occurrences that must finish before Main begins.
        self.state.flow = json!({"kind":"main","stage":"start-check"});
        self.checks()
    }

    pub(super) fn win_for_players(&mut self, players: Vec<String>, frame: &Frame) -> Result<()> {
        let mut winners = Vec::new();
        for seat in players {
            if !self.restricted(&format!("{seat}.leader"), "win")? {
                winners.push(seat);
            }
        }
        if !winners.is_empty() {
            self.state.game = json!({"ended":true,"winner":if winners.len()==1 {winners.first()} else {None},"by":"1.2.4"});
            let group = self.group();
            // Players made to win together are one result: no single one of them won.
            let player_batch = self.player_event_mark();
            for seat in winners {
                self.emit(
                    json!({"kind":"勝利","player":seat,"ability":frame.reference}),
                    &frame.cause,
                    group,
                );
            }
            self.seal_player_event_batch(player_batch);
        }
        Ok(())
    }

    pub(super) fn lose_for_players(&mut self, players: Vec<String>, frame: &Frame) -> Result<()> {
        let mut losers = Vec::new();
        for seat in players {
            if !self.restricted(&format!("{seat}.leader"), "lose")? {
                losers.push(seat);
            }
        }
        if losers.is_empty() {
            return Ok(());
        }
        // In a two-seat game, a single loss immediately gives the other seat the win.
        let winner = match losers.as_slice() {
            [loser] => Some(other(loser)),
            _ => None,
        };
        self.state.game = json!({"ended":true,"winner":winner,"by":"1.2.4"});
        let group = self.group();
        let player_batch = self.player_event_mark();
        for loser in losers {
            self.emit(
                json!({"kind":"敗北","player":loser,"ability":frame.reference}),
                &frame.cause,
                group,
            );
        }
        self.seal_player_event_batch(player_batch);
        Ok(())
    }

    pub(super) fn checks(&mut self) -> Result<()> {
        if self.state.game["ended"] == true {
            self.internal_terminal_cleanup();
            return Ok(());
        }
        let mut losers = Vec::new();
        for seat in ["P1", "P2"] {
            if self.restricted(&format!("{seat}.leader"), "lose")? {
                continue;
            }
            if int(&self.player(seat)?.leader["life"]) <= 0 {
                losers.push((seat, "11.2.1"));
                continue;
            }
            if self.state.draws_failed.contains(seat) {
                losers.push((seat, "11.2.2"));
            }
        }
        self.state.draws_failed.clear();
        if !losers.is_empty() {
            let ending_rule = if losers.len() == 2 {
                "1.2.2"
            } else {
                losers.first().map_or("11.2.1", |(_, rule)| *rule)
            };
            self.state.game = json!({"ended":true,"winner":if losers.len()==2 {None} else {losers.first().map(|(seat,_)|other(seat))},"by":ending_rule});
            let group = self.group();
            // Players defeated at the same check lose together: neither result alone says
            // that the other player did not win.
            let player_batch = self.player_event_mark();
            for (seat, rule) in losers {
                self.emit(
                    json!({"kind":"敗北","player":seat,"by":format!("rule-{rule}")}),
                    &json!({"rule":rule}),
                    group,
                );
            }
            self.seal_player_event_batch(player_batch);
            self.internal_terminal_cleanup();
            return Ok(());
        }
        if self.clean_attachment_rules()? || self.clean_field_rules()? {
            return self.checks();
        }
        if !self.card_runtime.has_pending() && self.state.flow["stage"] == "start-check" {
            self.start_main()?;
        }
        if !self.card_runtime.has_pending()
            && self.state.flow["kind"] == "end"
            && self.state.flow["stage"] == "triggers"
        {
            let mut guards = false;
            for id in self.zone_ids(self.active(), "field") {
                if self.object(&id)?.state["acted"] != true && self.keywords(&id)?.contains("guard")
                {
                    guards = true;
                    break;
                }
            }
            self.state.flow["stage"] = json!(if guards { "guard" } else { "quick" });
        }
        if !self.card_runtime.has_pending()
            && self.state.flow["kind"] == "end"
            && self.state.flow["stage"] == "discard"
            && self.hand_excess(self.active())? == 0
        {
            self.next_turn()?;
            return self.checks();
        }
        Ok(())
    }
}
