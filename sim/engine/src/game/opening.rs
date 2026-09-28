#![expect(
    clippy::indexing_slicing,
    reason = "Opening flow is a constructed, serializable protocol state."
)]
use super::legal::permutations;
use super::{Frame, Game, int, list, other, string};
use crate::{EngineFailure, Result, invalid};
use serde_json::{Value, json};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Opening procedures operate on the same authoritative game state."
)]
impl Game {
    pub(super) fn prepare_opening(setup: &Value) -> Value {
        let mut position = setup.clone();
        if setup["pregame"] != true {
            return position;
        }
        position["history"] = json!("none");
        position["turn"] =
            json!({"active":"P1","phase":"pregame","elapsed_turns":{"P1":0_i64,"P2":0_i64}});
        for seat in ["P1", "P2"] {
            position["players"][seat]["zones"] = json!({"deck":setup["players"][seat]["deck_list"],"evolve_deck":setup["players"][seat]["evolve_deck_list"]});
            position["players"][seat]["pp"] = json!({"current":0_i64,"max":0_i64});
            position["players"][seat]["leader"]["life"] = json!(20_i64);
        }
        position
    }

    pub(super) fn initialize_opening(&mut self) -> Result<()> {
        let mut queue = Vec::new();
        for seat in ["P1", "P2"] {
            let player = self.player(seat)?;
            let vanguard =
                player.construction == "title" && player.title == "カードファイト!! ヴァンガード";
            let idol = player.construction == "title"
                && player.title == "アイドルマスター シンデレラガールズ";
            if vanguard && !self.start_amulets(seat)?.is_empty() {
                queue.push(json!(seat));
                for id in self.start_amulets(seat)? {
                    self.learn(seat, &id, true);
                }
            }
            if idol {
                let frame = Frame {
                    controller: seat.into(),
                    cause: json!({"rule":"14.3.1.2"}),
                    ..Frame::default()
                };
                for _ in 0_u8..5 {
                    let id = self.new_named_object("魔法のアイテム", seat)?;
                    self.move_objects(&[id], "ex", None, None, &frame)?;
                }
            }
        }
        self.state.flow = json!({"kind":"pregame","stage":if queue.is_empty(){"first"}else{"amulet"},"queue":queue,"selections":{}});
        if self.state.flow["stage"] == "first" {
            self.shuffle_opening_decks()?;
        }
        Ok(())
    }

    fn start_amulets(&self, seat: &str) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for id in self.zone_ids(seat, "deck") {
            if self.keywords(&id)?.contains("start_amulet") {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    pub(super) fn opening_point(&self) -> Value {
        let by = match string(&self.state.flow["stage"]) {
            "amulet" => self.state.flow["queue"][0].clone(),
            "first" => self
                .state
                .random
                .get("first_chooser")
                .cloned()
                .unwrap_or_else(|| json!("P1")),
            _ => self.state.flow["mulligan_by"].clone(),
        };
        json!({"at":"pregame","by":by})
    }

    pub(super) fn opening_choices(&self) -> Result<Vec<Value>> {
        let point = self.opening_point();
        let seat = string(&point["by"]);
        Ok(match string(&self.state.flow["stage"]) {
            "amulet" => self
                .start_amulets(seat)?
                .into_iter()
                .map(|id| json!({"do":"choose-start-amulet","object":id}))
                .collect(),
            "first" => ["P1", "P2"]
                .into_iter()
                .map(|first| json!({"do":"choose-first","first":first}))
                .collect(),
            _ => {
                let mut options = vec![json!({"do":"mulligan","redo":false})];
                for order in permutations(&self.zone(seat, "hand")) {
                    options.push(json!({"do":"mulligan","redo":true,"order":order}));
                }
                options
            }
        })
    }

    pub(super) fn opening_decision(&mut self, decision: &Value) -> Result<bool> {
        if !self
            .opening_choices()?
            .iter()
            .any(|choice| super::legal::decision_matches(choice, decision))
        {
            return Ok(false);
        }
        let seat = string(&decision["by"]);
        match string(&decision["do"]) {
            "choose-start-amulet" => {
                let id = string(&decision["object"]);
                self.place_start_amulet(seat, id)?;
                self.state.flow["selections"][seat] = json!(id);
                let mut queue = list(&self.state.flow["queue"]);
                queue.remove(0);
                self.state.flow["queue"] = json!(queue);
                if queue.is_empty() {
                    self.state.flow["stage"] = json!("first");
                    self.shuffle_opening_decks()?;
                }
            }
            "choose-first" => {
                self.state.turn["first_player"] = decision["first"].clone();
                self.state.turn["active"] = decision["first"].clone();
                for controller in ["P1", "P2"] {
                    let frame = Frame {
                        controller: controller.into(),
                        cause: json!({"rule":"6.2.1.7"}),
                        ..Frame::default()
                    };
                    self.deal_opening(controller, &frame)?;
                }
                self.state.flow["stage"] = json!("mulligan");
                self.state.flow["mulligan_by"] = decision["first"].clone();
            }
            "mulligan" => {
                if decision["redo"] == true {
                    let frame = Frame {
                        controller: seat.into(),
                        cause: json!({"rule":"6.2.1.8"}),
                        ..Frame::default()
                    };
                    let order = list(&decision["order"])
                        .iter()
                        .map(|id| string(id).into())
                        .collect::<Vec<_>>();
                    self.move_objects(&order, "deck", None, Some(&json!("bottom")), &frame)?;
                    self.deal_opening(seat, &frame)?;
                }
                if self.state.turn["first_player"] == seat {
                    self.state.flow["mulligan_by"] = json!(other(seat));
                } else {
                    self.finish_opening()?;
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn place_start_amulet(&mut self, seat: &str, id: &str) -> Result<()> {
        self.player_mut(seat)?
            .zones
            .entry("deck".into())
            .or_default()
            .retain(|value| value != id);
        self.player_mut(seat)?
            .zones
            .entry("field".into())
            .or_default()
            .push(json!(id));
        let object = self.object_mut(id)?;
        object.zone = "field".into();
        object.generation = object.generation.saturating_add(1);
        object.state["pregame_hidden"] = json!(true);
        object.state["face_up"] = json!(false);
        self.learn(seat, id, true);
        let group = self.group();
        self.emit(
            json!({"kind":"場に出す","object":id,"from":format!("{seat}.deck"),"to":format!("{seat}.field"),"face_up":false}),
            &json!({"rule":"14.4.3.1"}),
            group,
        );
        Ok(())
    }

    fn deal_opening(&mut self, seat: &str, frame: &Frame) -> Result<()> {
        let mut remaining = 4_i64;
        let mut ids = Vec::new();
        let mut unknown = 0_i64;
        let mut deck = Vec::new();
        for item in self.zone(seat, "deck") {
            if remaining == 0 {
                deck.push(item);
                continue;
            }
            if let Some(id) = item.as_str() {
                ids.push(id.to_owned());
                remaining = remaining.saturating_sub(1);
            } else {
                let count = int(&item["filler"]);
                let take = count.min(remaining);
                remaining = remaining.saturating_sub(take);
                unknown = unknown.saturating_add(take);
                if count > take {
                    deck.push(json!({"filler":count.saturating_sub(take)}));
                }
            }
        }
        if remaining > 0 {
            return Err(invalid("opening deck has fewer than four cards"));
        }
        self.move_objects(&ids, "hand", None, None, frame)?;
        self.player_mut(seat)?.zones.insert("deck".into(), deck);
        if unknown > 0 {
            self.player_mut(seat)?
                .zones
                .entry("hand".into())
                .or_default()
                .push(json!({"filler":unknown}));
        }
        Ok(())
    }

    fn finish_opening(&mut self) -> Result<()> {
        let first = string(&self.state.turn["first_player"]).to_owned();
        for seat in ["P1", "P2"] {
            let player = self.player_mut(seat)?;
            player.pp = json!({"current":i64::from(seat==first),"max":i64::from(seat==first)});
            player.ep = if seat == first { 0 } else { 3 };
            player.sep = 1;
            player.leader["life"] = json!(20_i64);
            for id in self.zone_ids(seat, "field") {
                self.object_mut(&id)?.state["pregame_hidden"] = json!(false);
                self.object_mut(&id)?.state["face_up"] = json!(true);
                self.learn("P1", &id, true);
                self.learn("P2", &id, true);
            }
        }
        self.state.turn["elapsed_turns"][&first] = json!(1_i64);
        self.state.turn["phase"] = json!("main");
        self.state.flow = json!({"kind":"main"});
        let pending = self.collect_triggers("main_start", &[], &json!({"rule":"7.3"}))?;
        self.enqueue(pending);
        Ok(())
    }

    fn shuffle_opening_decks(&mut self) -> Result<()> {
        for seat in ["P1", "P2"] {
            let deck = self.zone(seat, "deck");
            let script = self.state.random["shuffles"]
                .as_array()
                .and_then(|entries| entries.get(self.state.random_index))
                .cloned();
            if let Some(script) = script {
                if script["player"] != seat || script["zone"] != "deck" {
                    return Err(invalid("opening shuffle belongs to another zone"));
                }
                let result = list(&script["result"]);
                let multiset = |items: &[Value]| {
                    let mut ids = items
                        .iter()
                        .filter(|value| value.is_string())
                        .map(Value::to_string)
                        .collect::<Vec<_>>();
                    ids.sort();
                    let fillers = items.iter().map(|value| int(&value["filler"])).sum::<i64>();
                    (ids, fillers)
                };
                if multiset(&deck) != multiset(&result) {
                    return Err(invalid("opening shuffle changed the card multiset"));
                }
                self.player_mut(seat)?.zones.insert("deck".into(), result);
                self.state.random_index = self.state.random_index.saturating_add(1);
                let ids = self.zone_ids(seat, "deck");
                for knowledge in self.state.knowledge.values_mut() {
                    for id in &ids {
                        knowledge.located.remove(id);
                    }
                }
            } else {
                if deck.iter().any(Value::is_object) {
                    return Err(EngineFailure::Unsupported(
                        "anonymous pregame cards require a scripted shuffle".into(),
                    ));
                }
                self.shuffle(seat, &self.zone_ids(seat, "deck"))?;
            }
        }
        Ok(())
    }
}
