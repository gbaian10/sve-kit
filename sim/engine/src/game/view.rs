#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use core::fmt::Write as _;

use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{Game, View, int, string};
use crate::{Result, invalid};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    pub(super) fn learn(&mut self, seat: &str, id: &str, located: bool) {
        let Some(object) = self.state.objects.get(id) else {
            return;
        };
        let knowledge = self.state.knowledge.entry(seat.into()).or_default();
        knowledge.seen.entry(id.into()).or_insert_with(
            || json!({"id":id,"card":object.card,"owner":object.owner,"from":self.node}),
        );
        if located {
            knowledge.located.insert(id.into());
        }
    }

    /// The actual outgoing packet. No adapter applies a second visibility filter.
    ///
    /// # Errors
    /// A visible card is not in the supplied snapshot or its effects cannot be inspected.
    pub fn projection(&self, view: View) -> Result<Value> {
        let seat = view.player();
        let mut packet = json!({"version":"astra-state/1","turn":self.state.turn,"room":self.state.room,"game":self.state.game,"objects":{}});
        for player in ["P1", "P2"] {
            let data = self.player(player)?;
            let mut value = json!({"leader":data.leader,"pp":data.pp,"ep":data.ep,"sep":data.sep,"construction":data.construction,"title":data.title});
            if (self.state.room["open_decklists"] == true || seat.is_none() || seat == Some(player))
                && !data.deck_list.is_null()
            {
                value["deck_list"] = data.deck_list.clone();
            }
            for (zone, items) in &data.zones {
                if zone == "void" {
                    continue;
                }
                let mut visible = Vec::new();
                for item in items {
                    let Some(id) = item.as_str() else {
                        visible.push(item.clone());
                        continue;
                    };
                    let can_see = seat.is_none_or(|seat| {
                        self.state
                            .knowledge
                            .get(seat)
                            .is_some_and(|k| k.located.contains(id))
                    });
                    if can_see {
                        let object = self.object(id)?;
                        let mut card = object.state.clone();
                        card["id"] = json!(id);
                        card["card"] = json!(object.card);
                        card["generation"] = json!(object.generation);
                        card["damage"] =
                            json!(int(&card["max_hp"]).saturating_sub(int(&card["hp"])));
                        if zone == "field" {
                            card["keywords"] = json!(
                                self.keywords(id)?
                                    .iter()
                                    .map(|id| self.catalog.keyword_name(id))
                                    .collect::<Vec<_>>()
                            );
                        }
                        packet["objects"][id] = card;
                        visible.push(json!(id));
                    } else {
                        visible.push(json!({"filler":1_i64}));
                    }
                }
                value[zone] = json!(visible);
                value[format!("{zone}_count")] = json!(self.zone_count(player, zone));
            }
            packet[player] = value;
        }
        packet["semantic_state"] = self.semantic_projection(view);
        let knowledge = seat.and_then(|seat| self.state.knowledge.get(seat));
        packet["knowledge"] = knowledge.map_or_else(||json!({"identifiable":self.state.objects.keys().collect::<Vec<_>>(),"carried":[]}),|knowledge|json!({"identifiable":knowledge.seen.keys().collect::<Vec<_>>(),"located":knowledge.located,"carried":knowledge.carried}));
        packet["known_cards"] = knowledge.map_or_else(||json!([]),|knowledge| {
            json!(
                knowledge
                    .seen
                    .values()
                    .map(|value| json!({"id":value["id"],"card":value["card"],"owner":value["owner"]}))
                    .collect::<Vec<_>>()
            )
        });
        packet["awaiting"] = self.awaited(view)?;
        if seat.is_none_or(|seat| self.input_point()["by"] == seat) {
            packet["legal"] = json!(self.legal()?);
        }
        packet["flow"] = self.state.flow.clone();
        normalize_origins(&mut packet["flow"]);
        if let Some(frame) = &self.state.frame
            && seat.is_none_or(|seat| frame.controller == seat)
        {
            let mut continuation = json!({"frame":frame,"prompt":self.state.prompt});
            continuation["frame"]["cause"] = Value::Null;
            if self.visible_references(&continuation, view) {
                packet["continuation"] = continuation;
            }
        }
        Ok(packet)
    }

    fn semantic_projection(&self, view: View) -> Value {
        let seat = view.player();
        let pending=self.state.pending.iter().map(|pending|{
            let mut value=json!({"controller":pending.controller,"ability":pending.reference,"event":pending.event});
            if let Some(id)=&pending.id {value["id"]=json!(id);}
            if let Some(context)=&pending.context {
                let context=json!(context);
                if self.visible_references(&context,view) {value["program"]=pending.code.clone();value["context"]=context;}
            }
            value
        }).collect::<Vec<_>>();
        let mut semantic =
            json!({"pending_triggers":pending,"counters_this_turn":self.state.counters});
        semantic["continuous_effects"] = json!(
            self.state
                .continuous
                .iter()
                .filter(|entry| self.visible_references(entry, view))
                .collect::<Vec<_>>()
        );
        semantic["delayed_triggers"] = json!(
            self.state
                .delayed
                .iter()
                .filter(|entry| self.visible_references(entry, view))
                .collect::<Vec<_>>()
        );
        semantic["used_this_turn"] = json!({});
        for (key, count) in &self.state.used {
            if seat.is_none()
                || serde_json::from_str::<Value>(key)
                    .is_ok_and(|entry| self.visible_references(&entry, view))
            {
                semantic["used_this_turn"][key] = json!(count);
            }
        }
        semantic
    }

    fn visible_references(&self, value: &Value, view: View) -> bool {
        match value {
            Value::String(id) if self.state.objects.contains_key(id) => {
                view.player().is_none_or(|seat| {
                    self.state
                        .knowledge
                        .get(seat)
                        .is_some_and(|k| k.seen.contains_key(id))
                })
            }
            Value::Array(items) => items.iter().all(|item| self.visible_references(item, view)),
            Value::Object(fields) => fields.iter().all(|(key, member)| {
                self.visible_references(&json!(key), view) && self.visible_references(member, view)
            }),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => true,
        }
    }

    fn awaited(&self, view: View) -> Result<Value> {
        if self.state.game["ended"] == true {
            return Ok(Value::Null);
        }
        let point = self.input_point();
        let seat = string(&point["by"]);
        if view.player().is_some_and(|viewer| viewer != seat) {
            return Ok(json!({"by":seat}));
        }
        let choices = if let Some(prompt) = &self.state.prompt {
            prompt.choices.clone()
        } else if self.pending_player().is_some() {
            self.state
                .pending
                .iter()
                .filter(|p| p.controller == seat)
                .map(Self::pending_choice)
                .collect()
        } else {
            self.legal()?
        };
        Ok(json!({"by":seat,"at":point["at"],"choices":choices}))
    }

    /// Reads a path from the exact same packet used by clients.
    ///
    /// # Errors
    /// When packet serialization cannot inspect a visible card.
    pub fn query(&self, view: View, path: &str) -> Result<Option<Value>> {
        Ok(Self::packet_query(&self.projection(view)?, path))
    }

    /// Traverses object fields or a zone member identified by its public id.
    #[must_use]
    pub fn packet_query(packet: &Value, path: &str) -> Option<Value> {
        let mut current = packet;
        for part in path.split('.') {
            current = if let Some(items) = current.as_array() {
                {
                    items.iter().find(|item| item.as_str() == Some(part))?;
                    packet.get("objects")?.get(part)?
                }
            } else {
                current.get(part)?
            };
        }
        Some(current.clone())
    }

    /// Semantic hash of all authoritative rule state, including continuations and randomness.
    ///
    /// # Errors
    /// When serialization fails.
    pub fn digest(&self) -> Result<String> {
        let mut value = serde_json::to_value(&self.state).map_err(invalid)?;
        normalize_origins(&mut value);
        let bytes = serde_json::to_vec(&json!({"version":"astra-digest/1","state":value}))
            .map_err(invalid)?;
        let mut hex = String::new();
        for byte in Sha256::digest(bytes) {
            write!(hex, "{byte:02x}").map_err(invalid)?;
        }
        Ok(hex)
    }

    pub(crate) fn carry_from(&mut self, source: &Self) {
        for seat in ["P1", "P2"] {
            let Some(known) = source.state.knowledge.get(seat) else {
                continue;
            };
            let knowledge = self.state.knowledge.entry(seat.into()).or_default();
            for (id, card) in &known.seen {
                if !knowledge.carried.iter().any(|entry| entry["object"] == *id) {
                    knowledge.seen.insert(id.clone(), card.clone());
                    knowledge
                        .carried
                        .push(json!({"object":id,"from":card["from"]}));
                }
            }
        }
    }

    pub(crate) fn opening_node(&mut self, node: &str) {
        self.node = node.into();
        for knowledge in self.state.knowledge.values_mut() {
            for seen in knowledge.seen.values_mut() {
                if seen["from"] == "opening" {
                    seen["from"] = json!(node);
                }
            }
        }
    }
}

fn normalize_origins(root: &mut Value) {
    match root {
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(key.as_str(), "cause" | "from")
                    && (value.is_object() || value.is_string())
                {
                    *value = json!("origin");
                } else {
                    normalize_origins(value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_origins(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}
