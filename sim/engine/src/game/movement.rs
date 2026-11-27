#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use super::{Frame, Game, Object, int, list, scalar, string};
use crate::ir::{Position, SnapshotId};
use crate::{Result, invalid};
use alloc::collections::{BTreeMap, BTreeSet};
use core::slice::from_ref;
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Attachment {
    Equipment,
    LimitBreak,
}

impl Attachment {
    pub(super) const fn zone(self) -> &'static str {
        match self {
            Self::Equipment => "equipment",
            Self::LimitBreak => "limit_break",
        }
    }

    pub(super) const fn link(self) -> &'static str {
        match self {
            Self::Equipment => "equipped_to",
            Self::LimitBreak => "limit_break_of",
        }
    }

    pub(super) const fn erasure_rule(self) -> &'static str {
        match self {
            Self::Equipment => "11.11.1",
            Self::LimitBreak => "11.12.1",
        }
    }

    pub(super) const fn controller_rule(self) -> &'static str {
        match self {
            Self::Equipment => "11.11.2",
            Self::LimitBreak => "11.12.2",
        }
    }
}

impl Game {
    pub(super) fn emit_zone_movement(
        &mut self,
        previous: &Object,
        destination: &str,
        position: Option<Position>,
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
        if destination == "deck"
            && let Some(position) = position
        {
            event["position"] = match position {
                Position::Top {} => json!(1_u8),
                Position::Bottom {} => json!("bottom"),
                Position::Index { index } => json!(index.0.get()),
            };
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
        let before = SnapshotId(self.state.ir_epoch);
        let player_batch = self.player_event_mark();
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
        self.seal_player_event_batch(player_batch);
        self.internal_posture_events(&changed, acted, cause, before)?;
        if !changed.is_empty() {
            self.require_ability_sources(if acted {
                "event act subscriptions"
            } else {
                "event stand subscriptions"
            })?;
        }
        self.group();
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

    pub(super) fn discard(&mut self, ids: &[String], frame: &Frame) -> Result<()> {
        let discarded = ids
            .iter()
            .filter_map(|id| self.state.objects.get(id))
            .filter(|object| object.zone == "hand")
            .cloned()
            .collect::<Vec<_>>();
        let hand_traits = discarded
            .iter()
            .map(|object| self.card_traits(&object.id))
            .collect::<Result<Vec<_>>>()?;
        let group = self.group();
        for object in &discarded {
            self.internal_record_discard_history(&object.controller);
            self.emit(
                json!({"kind":"捨てる","player":object.controller,"object":object.id}),
                &frame.cause,
                group,
            );
        }
        if !discarded.is_empty() {
            self.require_ability_sources("event discard subscriptions")?;
        }
        let actual = discarded
            .iter()
            .map(|object| object.id.clone())
            .collect::<Vec<_>>();
        let player_mark = self.player_event_mark();
        self.move_objects(&actual, "cemetery", None, None, frame)?;
        for (previous, traits) in discarded.iter().zip(hand_traits) {
            if self.object(&previous.id)?.generation != previous.generation {
                self.internal_record_discard_traits(&previous.controller, traits);
            }
        }
        self.player_movement_method(
            player_mark,
            &actual,
            super::player::events::MovementMethod::Discard,
        );
        self.group();
        Ok(())
    }

    // Birth and evolution must disclose the selected face without learning a temporary face zero.
    pub(super) fn movement_face(previous: &Object, destination: &str) -> Result<usize> {
        let face = if previous.zone == "void"
            || (previous.zone == "evolve_deck" && destination == "evolution")
        {
            previous.state["face"]
                .as_u64()
                .ok_or_else(|| invalid("unknown movement face"))?
        } else {
            0
        };
        usize::try_from(face).map_err(invalid)
    }

    pub(super) fn moved_attributes(
        previous: &Object,
        printed: &Value,
        destination: &str,
    ) -> Result<Value> {
        if matches!(previous.zone.as_str(), "ex" | "resolution")
            && matches!(destination, "resolution" | "field")
        {
            return Ok(previous.state.clone());
        }
        let face = Self::movement_face(previous, destination)?;
        let acted = destination == "field" && previous.state["acted"] == true;
        let mut attributes = json!({"power":scalar(&printed["power"]),"hp":scalar(&printed["hp"]),"max_hp":scalar(&printed["hp"]),"acted":acted,"evolved":false,"entered_this_turn":false,"face":face,"damage":0_i64,"counters":{},"keywords":[],"silenced":false,"stats_increased_this_turn":false,"attacks_this_turn":0_i64});
        if destination == "banish" {
            attributes["face_up"] = json!(true);
        }
        Ok(attributes)
    }

    pub(super) fn single_rules_name_matches_role(&self, id: &str, role: &str) -> Result<bool> {
        self.catalog
            .rule_bindings
            .matches_resource(role, &[self.card_name(id)?])
    }

    pub(super) fn card_matches_resource(&self, id: &str, role: &str) -> Result<bool> {
        Ok(self.single_rules_name_matches_role(id, role)?
            || self
                .catalog
                .rule_bindings
                .matches_resource(role, &self.card_names(id)?)?)
    }

    pub(super) fn card_names(&self, id: &str) -> Result<Vec<String>> {
        let source = self.information_source(id)?;
        let face = usize::try_from(int(&source.state["face"])).map_err(invalid)?;
        self.card_names_for_print(id, &source.card, face)
    }

    pub(super) fn card_names_for_print(
        &self,
        id: &str,
        card: &str,
        face: usize,
    ) -> Result<Vec<String>> {
        let object = self.object(id)?;
        let mut names = self.printed_card_names(card, face, &object.zone)?;
        if object.state["silenced"] == true || self.internal_suppresses_printed(id, card, face) {
            names.truncate(1);
        }
        Ok(names)
    }

    pub(super) fn card_name_matches(
        &self,
        id: &str,
        predicate: impl Fn(&str) -> bool,
    ) -> Result<bool> {
        let source = self.information_source(id)?;
        let face = usize::try_from(int(&source.state["face"])).map_err(invalid)?;
        self.card_name_matches_print(id, &source.card, face, predicate)
    }

    pub(super) fn card_name_matches_print(
        &self,
        id: &str,
        card: &str,
        face: usize,
        predicate: impl Fn(&str) -> bool,
    ) -> Result<bool> {
        // An additional name cannot invalidate a match already proven by the printed name.
        if predicate(&self.catalog.rules_label(card, face)?) {
            return Ok(true);
        }
        Ok(self
            .card_names_for_print(id, card, face)?
            .iter()
            .any(|name| predicate(name)))
    }

    pub(super) fn printed_card_names(
        &self,
        card: &str,
        face: usize,
        zone: &str,
    ) -> Result<Vec<String>> {
        let mut names = vec![self.catalog.rules_label(card, face)?];
        if !self.card_runtime.is_empty() {
            names.extend(self.internal_printed_name_aliases(card, face, zone == "field")?);
        }
        let mut seen = BTreeSet::new();
        names.retain(|name| seen.insert(name.clone()));
        Ok(names)
    }

    pub(super) fn field_ids(&self) -> Vec<String> {
        ["P1", "P2"]
            .iter()
            .flat_map(|seat| self.zone_ids(seat, "field"))
            .collect()
    }

    pub(super) fn card_name(&self, id: &str) -> Result<String> {
        let object = self.information_source(id)?;
        let face = usize::try_from(int(&object.state["face"])).map_err(invalid)?;
        self.catalog.rules_label(&object.card, face)
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
            self.require_ability_sources("movement replacement")?;
            let destination = if self.internal_banish_replaces_move(id, zone)? {
                "banish".into()
            } else {
                zone.to_owned()
            };
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

    pub(super) fn return_advances(&mut self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let frame = Frame {
            cause: json!({"rule":"9.2.2"}),
            ..Frame::default()
        };
        self.move_objects(ids, "evolve_deck", None, None, &frame)?;
        for id in ids {
            self.object_mut(id)?.state["face_up"] = json!(true);
        }
        for seat in ["P1", "P2"] {
            self.learn_members(seat, ids, true)?;
        }
        Ok(())
    }

    pub(super) fn attach_equipment(
        &mut self,
        holder: &str,
        equipment: &str,
        frame: &Frame,
    ) -> Result<()> {
        if self.object(holder)?.zone != "field"
            || !self.object_type(holder)?.contains("フォロワー")
            || self.object(equipment)?.zone != "equipment"
            || self.object_type(equipment)? != "イクイップメント・トークン"
        {
            return Err(invalid("equipment attachment endpoints"));
        }
        self.object_mut(equipment)?.state["equipped_to"] = json!(holder);
        let group = self.group();
        let player_step = self.emit(
            json!({"kind":"装備","object":holder,"equipment":equipment}),
            &frame.cause,
            group,
        );
        self.player_step_actor(&player_step, &frame.controller);
        Ok(())
    }

    pub(super) fn erase_attachment(&mut self, id: &str, kind: Attachment) -> Result<()> {
        let player_subject = self.player_subject_before(id);
        let object = self.object(id)?.clone();
        let generation = object
            .generation
            .checked_add(1)
            .ok_or_else(|| invalid("attachment generation exhausted"))?;
        self.player_mut(&object.controller)?
            .zones
            .entry(kind.zone().into())
            .or_default()
            .retain(|value| value != id);
        let attachment = self.object_mut(id)?;
        attachment.zone = "void".into();
        attachment.generation = generation;
        attachment.state[kind.link()] = Value::Null;
        let group = self.group();
        let player_step = self.emit(
            json!({"kind":"消去","object":id,"by":format!("rule-{}", kind.erasure_rule())}),
            &json!({"rule":kind.erasure_rule()}),
            group,
        );
        self.player_subject_after(&player_step, player_subject);
        Ok(())
    }

    pub(super) fn release_links(&mut self, leaving: &[Object], frame: &Frame) -> Result<()> {
        let mut resources = BTreeSet::new();
        let mut attachments = BTreeSet::new();
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
            for kind in [Attachment::Equipment, Attachment::LimitBreak] {
                attachments.extend(
                    self.state
                        .objects
                        .values()
                        .filter(|item| {
                            item.zone == kind.zone() && item.state[kind.link()] == object.id
                        })
                        .map(|item| (kind, item.id.clone())),
                );
            }
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
        for (kind, id) in attachments {
            self.erase_attachment(&id, kind)?;
        }
        Ok(())
    }
}
