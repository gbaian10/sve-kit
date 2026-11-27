#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use crate::ir::SnapshotId;
use alloc::collections::BTreeMap;
use core::slice::from_ref;
use serde_json::{Value, json};

use super::placement::Placement;
use super::{Frame, Game, Object, int, list, string};
use crate::{EngineFailure, Result, invalid};

pub(super) fn token_erasure_zone(kind: &str, destination: &str) -> bool {
    match kind {
        "イクイップメント・トークン" => destination != "equipment",
        super::limit_break::TREASURE => !matches!(destination, "ex" | "limit_break"),
        "クレスト・トークン" => destination != "ex",
        "スペル・トークン" => !matches!(destination, "ex" | "resolution"),
        _ => kind.contains("トークン") && !matches!(destination, "field" | "ex" | "resolution"),
    }
}

impl Game {
    pub(super) fn move_objects(
        &mut self,
        ids: &[String],
        zone: &str,
        side: Option<&str>,
        position: Option<&Value>,
        frame: &Frame,
    ) -> Result<()> {
        self.move_objects_with_play_grants(ids, zone, side, position, frame, false)
    }

    pub(super) fn move_playing_card(
        &mut self,
        holder: &str,
        zone: &str,
        frame: &Frame,
    ) -> Result<()> {
        self.move_objects_with_play_grants(
            &[holder.into()],
            zone,
            Some(&frame.controller),
            None,
            frame,
            true,
        )
    }

    pub(in crate::game) fn move_effect_playing_card(
        &mut self,
        holder: &str,
        controller: &str,
        destination: &str,
        frame: &Frame,
    ) -> Result<()> {
        self.move_objects_with_play_grants(
            &[holder.into()],
            destination,
            Some(controller),
            None,
            frame,
            destination == "field",
        )
    }

    fn commit_movement_grants(
        &mut self,
        id: &str,
        generation: u64,
        from: &str,
        destination: &str,
        carry_grants: bool,
    ) -> Result<()> {
        if carry_grants || (from == "ex" && destination == "field") {
            // Arriving characteristics and entry observers must see carried effects.
            self.internal_commit_rule_carry(id, generation)?;
        }
        Ok(())
    }

    fn rule_carry_movement_ready(
        &self,
        ids: &[String],
        destinations: &BTreeMap<String, String>,
        carry_grants: bool,
    ) -> Result<()> {
        for id in ids {
            if carry_grants
                || (self.object(id)?.zone == "ex"
                    && destinations
                        .get(id)
                        .is_some_and(|destination| destination == "field"))
            {
                self.internal_rule_carry_ready(id)?;
            }
        }
        Ok(())
    }

    fn check_departure_subscriptions(
        &self,
        leaving: &[Object],
        destinations: &BTreeMap<String, String>,
    ) -> Result<()> {
        if !leaving.is_empty() {
            self.require_ability_sources("event leave subscriptions")?;
        }
        if leaving.iter().any(|object| {
            destinations
                .get(&object.id)
                .is_some_and(|destination| destination == "cemetery")
        }) {
            self.require_ability_sources("event field_to_cemetery subscriptions")?;
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Simultaneous leave snapshots, moves, token rules and entry triggers share one transaction."
    )]
    fn move_objects_with_play_grants(
        &mut self,
        ids: &[String],
        zone: &str,
        side: Option<&str>,
        position: Option<&Value>,
        frame: &Frame,
        carry_grants: bool,
    ) -> Result<()> {
        let player_batch = self.player_event_mark();
        let mut movable = Vec::new();
        for id in ids {
            if self.stack_replaces_departure(id, zone)? {
                let count = int(&self.object(id)?.state["counters"]["stack_counter"]);
                self.object_mut(id)?.state["counters"]["stack_counter"] =
                    json!(count.saturating_sub(1));
                let group = self.group();
                self.emit(json!({"kind":"取代","object":id,"original":"場を離れる（墓場に置く）","replacement":"スタックカウンターを1個取り除き、場に残る"}),&frame.cause,group);
                self.emit(
                    json!({"kind":"カウンター","object":id,"name":self.catalog.counter_name("stack_counter"),"delta":-1_i64,"before":count,"after":count.saturating_sub(1)}),
                    &frame.cause,
                    group,
                );
            } else {
                movable.push(id.clone());
            }
        }
        let typed_entry_before = SnapshotId(self.state.ir_epoch);
        let destinations = self.movement_destinations(&movable, zone, frame)?;
        self.rule_carry_movement_ready(&movable, &destinations, carry_grants)?;
        self.emit_banish_instruction(&movable, &destinations, frame)?;
        let leaving = movable
            .iter()
            .filter_map(|id| self.state.objects.get(id))
            .filter(|o| o.zone == "field" && zone != "field")
            .cloned()
            .collect::<Vec<_>>();
        self.check_departure_subscriptions(&leaving, &destinations)?;
        let group = self.group();
        let mut learned: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut entered = Vec::new();
        let mut erased = Vec::new();
        let mut advances = Vec::new();
        let ex_departures =
            self.internal_before_ex_departures(&movable, typed_entry_before, frame)?;
        let typed_departures =
            self.internal_before_departures(&movable, &destinations, side, frame)?;
        let banish_zone = self.internal_before_banish_zone(&movable, &destinations, frame)?;
        let player_before = self.player_movement_before(&movable);
        let plans = self.place_zone_batch(&movable, &destinations, side, position, frame)?;
        for plan in &plans {
            if !plan.changed {
                continue;
            }
            let previous = &plan.previous;
            let id = &previous.id;
            let destination = plan.destination.as_str();
            let owner = &plan.controller;
            let face = Self::movement_face(previous, destination)?;
            let initial_acted = destination == "field"
                && previous.zone != "field"
                && self.internal_entry_acted(&previous.card, face)?;
            let printed = self.catalog.face(&previous.card, face)?.clone();
            let object = self.object_mut(id)?;
            object.zone = destination.into();
            object.controller.clone_from(owner);
            if previous.zone != destination || previous.controller != *owner {
                object.generation = object.generation.saturating_add(1);
                object.state = Self::moved_attributes(previous, &printed, destination)?;
            }
            let from = format!("{}.{}", previous.controller, previous.zone);
            if destination == "field" {
                if initial_acted || frame.values.get("field_entry_acted") == Some(&json!(true)) {
                    object.state["acted"] = json!(true);
                }
                object.state["entered_this_turn"] = json!(true);
                object.state["entered_from"] = if previous.zone == "resolution" {
                    previous.state["entered_from"].clone()
                } else if matches!(
                    previous.zone.as_str(),
                    "hand" | "ex" | "cemetery" | "deck" | "evolve_deck"
                ) {
                    json!(previous.zone)
                } else {
                    json!("other")
                };
                object.state["entered_by"] = json!(if previous.zone == "resolution" {
                    "play"
                } else {
                    "effect"
                });
                let mut event = json!({"kind":"場に出す","object":id,"from":from,"to":format!("{owner}.field")});
                if previous.zone != "void" || previous.state["creation_print_selected"] == true {
                    event["card"] = json!(previous.card);
                }
                let event_id = self.emit(event, &frame.cause, group);
                entered.push((self.object(id)?.clone(), json!({"event":event_id})));
            } else {
                self.emit_zone_movement(previous, destination, position, frame, group)?;
            }
            self.commit_movement_grants(
                id,
                previous.generation,
                &previous.zone,
                destination,
                carry_grants,
            )?;
            if previous.zone != destination || previous.controller != *owner {
                self.state.ir_ability_suppression.remove(id);
                if let Some(count) = self.state.counters.get_mut(&format!("{id}.attacks")) {
                    *count = 0;
                }
            }
            self.collect_movement_knowledge(id, owner, destination, &mut learned);
            self.internal_record_lesson_history(id, previous, destination)?;
            if token_erasure_zone(string(&printed["card_type"]), destination) {
                erased.push(id.clone());
            }
            if string(&printed["card_type"]).contains("アドバンス")
                && !matches!(destination, "evolve_deck" | "field" | "ex" | "resolution")
            {
                advances.push(id.clone());
            }
        }
        for (viewer, members) in learned {
            self.learn_members(&viewer, &members, true)?;
        }
        for plan in &plans {
            if plan.changed && plan.previous.zone == "limit_break" {
                self.remove_limit_break_bonus(&plan.previous)?;
            }
        }
        self.internal_commit_departures(typed_departures)?;
        self.internal_numeric_cleanup()?;
        self.conceal_placement_order(&plans, frame);
        self.player_movement_after(player_before);
        self.seal_player_event_batch(player_batch);
        let erase_batch = self.player_event_mark();
        let erase_group = self.group();
        for id in erased {
            let player_subject = self.player_subject_before(&id);
            let object = self.object(&id)?.clone();
            if let Some(items) = self
                .player_mut(&object.controller)?
                .zones
                .get_mut(&object.zone)
            {
                items.retain(|v| v.as_str() != Some(&id));
            }
            let token = self.object_mut(&id)?;
            token.zone = "void".into();
            // Erasure invalidates every earlier reference to the token (CR 9.1.3, 9.1.4.4).
            token.generation = token.generation.saturating_add(1);
            let player_step = self.emit(
                json!({"kind":"消去","object":id,"by":"rule-9.1.4.4"}),
                &json!({"rule":"9.1.4.4"}),
                erase_group,
            );
            self.player_subject_after(&player_step, player_subject);
        }
        self.seal_player_event_batch(erase_batch);
        self.return_advances(&advances)?;
        self.release_links(&leaving, frame)?;
        if plans
            .iter()
            .any(|plan| plan.changed && plan.destination == "ex")
        {
            self.require_ability_sources("event ex_enter subscriptions")?;
        }
        let ex_previous = plans
            .iter()
            .filter(|plan| {
                plan.changed
                    && plan.destination == "ex"
                    && (plan.previous.zone != "ex" || plan.previous.controller != plan.controller)
            })
            .map(|plan| plan.previous.clone())
            .collect::<Vec<_>>();
        self.internal_ex_events(&ex_previous, typed_entry_before, frame)?;
        let milled = plans
            .iter()
            .filter(|plan| {
                plan.changed && plan.previous.zone == "deck" && plan.destination == "cemetery"
            })
            .map(|plan| plan.previous.clone())
            .collect::<Vec<_>>();
        self.internal_mill_events(&milled, typed_entry_before, frame)?;
        let banished = plans
            .iter()
            .filter(|plan| {
                plan.changed && plan.destination == "banish" && plan.previous.zone != "banish"
            })
            .map(|plan| plan.previous.id.clone())
            .collect::<Vec<_>>();
        self.internal_commit_banish_zone(banish_zone, &banished)?;
        let departed_ex = plans
            .iter()
            .filter(|plan| plan.changed && plan.previous.zone == "ex")
            .map(|plan| {
                (
                    plan.previous.clone(),
                    plan.destination.clone(),
                    plan.controller.clone(),
                )
            })
            .collect::<Vec<_>>();
        self.internal_ex_departure_events(ex_departures, &departed_ex, typed_entry_before, frame)?;
        self.group();
        self.finish_field_entries(&plans, &entered, typed_entry_before, frame)
    }

    fn finish_field_entries(
        &mut self,
        plans: &[Placement],
        entered: &[(Object, Value)],
        typed_entry_before: SnapshotId,
        frame: &Frame,
    ) -> Result<()> {
        for (object, _) in entered {
            self.initialize_entry_counters(&object.id, frame)?;
        }
        let entry_previous = plans
            .iter()
            .filter(|plan| {
                plan.changed && plan.destination == "field" && plan.previous.zone != "field"
            })
            .map(|plan| plan.previous.clone())
            .collect::<Vec<_>>();
        self.internal_record_field_entry_methods(&entry_previous, frame)?;
        self.internal_field_events(
            &entry_previous,
            typed_entry_before,
            frame,
            #[cfg(feature = "runner")]
            entered,
        )?;
        for _ in entered {
            self.require_ability_sources("event enter subscriptions")?;
            self.group();
        }
        Ok(())
    }

    /// `method` states whether a rule or an effect instruction draws: the move itself looks
    /// the same, and nothing recorded later can tell the two apart.
    pub(super) fn draw(
        &mut self,
        seat: &str,
        frame: &Frame,
        method: super::player::events::MovementMethod,
    ) -> Result<Option<String>> {
        // "カードを引けない": the draw does not happen; it is not a failed draw either.
        if self.restricted(&format!("{seat}.leader"), "draw")? {
            return Ok(None);
        }
        let deck = self.zone(seat, "deck");
        let Some(top) = deck.first() else {
            self.state.draws_failed.insert(seat.into());
            return Ok(None);
        };
        let id = top
            .as_str()
            .ok_or_else(|| EngineFailure::Unsupported("a filler was observed by drawing".into()))?
            .to_owned();
        let generation = self.object(&id)?.generation;
        let player_mark = self.player_event_mark();
        self.move_objects(from_ref(&id), "hand", None, None, frame)?;
        let actual = self.object(&id)?;
        if actual.generation == generation || actual.zone != "hand" || actual.controller != seat {
            return Ok(None);
        }
        self.player_movement_method(player_mark, from_ref(&id), method);
        let group = self.group();
        self.emit(
            json!({"kind":"引く","player":seat,"object":id}),
            &frame.cause,
            group,
        );
        Ok(Some(id))
    }

    fn collect_movement_knowledge(
        &self,
        id: &str,
        owner: &str,
        destination: &str,
        learned: &mut BTreeMap<String, Vec<String>>,
    ) {
        for viewer in ["P1", "P2"] {
            let was_known = self
                .state
                .knowledge
                .get(viewer)
                .is_some_and(|known| known.knows_location(id));
            if !matches!(destination, "hand" | "deck" | "evolve_deck")
                || (owner == viewer && destination == "hand")
                || was_known
            {
                learned.entry(viewer.into()).or_default().push(id.into());
            }
        }
    }

    pub(super) fn reveal(
        &mut self,
        ids: &[String],
        to: &str,
        frame: &Frame,
        located: bool,
    ) -> Result<()> {
        let player_batch = self.player_event_mark();
        let group = self.group();
        for seat in ["P1", "P2"] {
            if to == "all" || seat == to {
                self.learn_members(seat, ids, located)?;
            }
        }
        for id in ids {
            self.emit(
                json!({"kind":"公開","object":id,"to":to,"source":frame.source}),
                &frame.cause,
                group,
            );
        }
        self.seal_player_event_batch(player_batch);
        Ok(())
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
            let mut selected = indexes.iter().map(|&i| deck[i].clone()).collect::<Vec<_>>();
            self.state.rng.shuffle(&mut selected);
            for (index, value) in indexes.iter().zip(selected) {
                deck[*index] = value;
            }
        }
        self.player_mut(seat)?.zones.insert("deck".into(), deck);
        for knowledge in self.state.knowledge.values_mut() {
            for id in ids {
                knowledge.forget_location(id);
            }
        }
        Ok(())
    }
}
