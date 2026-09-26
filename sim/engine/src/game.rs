//! Authoritative state, deterministic transitions and player packets.

#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]

mod belief;
mod combat;
mod costs;
mod damage;
mod effects;
mod expr;
mod extensions;
mod grants;
mod keywords;
mod legal;
mod movement;
mod nested;
mod opening;
mod payments;
mod placement;
mod progression;
mod randomness;
mod resources;
mod restrictions;
mod rules;
mod selections;
mod state_rules;
mod statistics;
mod temporal;
mod tokens;
mod turns;
mod view;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::catalog::Catalog;
use crate::{EngineFailure, Result, invalid};

/// Seat or referee requesting an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Full server state, never sent to a player.
    Referee,
    /// First seat.
    P1,
    /// Second seat.
    P2,
}

impl View {
    pub(crate) const fn player(self) -> Option<&'static str> {
        match self {
            Self::Referee => None,
            Self::P1 => Some("P1"),
            Self::P2 => Some("P2"),
        }
    }
}

/// Outcome and causal events emitted by an authoritative transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// Resolution, rejection, pause, cancellation or game end.
    pub outcome: String,
    /// Immutable events with causal references and simultaneous group identities.
    pub events: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Object {
    id: String,
    card: String,
    owner: String,
    controller: String,
    zone: String,
    generation: u64,
    state: Value,
}

#[derive(Debug, Clone)]
struct EventSubject {
    id: String,
    attributes: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Player {
    leader: Value,
    pp: Value,
    ep: i64,
    sep: i64,
    construction: String,
    title: String,
    deck_list: Value,
    zones: BTreeMap<String, Vec<Value>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Knowledge {
    seen: BTreeMap<String, Value>,
    located: BTreeSet<String>,
    carried: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Pending {
    controller: String,
    reference: Value,
    event: Value,
    code: Value,
    source: String,
    cause: Value,
    retained: bool,
    id: Option<String>,
    context: Option<Frame>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Frame {
    source: String,
    controller: String,
    #[serde(default)]
    event: Value,
    #[serde(default)]
    values: BTreeMap<String, Value>,
    reference: Value,
    decision: Value,
    bindings: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    ordering: BTreeMap<String, String>,
    captured: BTreeMap<String, Value>,
    frozen: BTreeMap<String, Value>,
    todo: Vec<Value>,
    cause: Value,
    occurrence: u64,
    performed: i64,
    paid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Prompt {
    by: String,
    choices: Vec<Value>,
    resume: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AbilityUse {
    ability: Value,
    #[serde(default)]
    generation: u64,
    count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventOccurrence {
    event: String,
    subject: String,
    count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct State {
    players: BTreeMap<String, Player>,
    objects: BTreeMap<String, Object>,
    turn: Value,
    room: Value,
    pending: Vec<Pending>,
    frame: Option<Frame>,
    prompt: Option<Prompt>,
    flow: Value,
    counters: BTreeMap<String, i64>,
    continuous: Vec<Value>,
    delayed: Vec<Value>,
    used: BTreeMap<String, AbilityUse>,
    #[serde(default)]
    event_occurrences: BTreeMap<String, EventOccurrence>,
    knowledge: BTreeMap<String, Knowledge>,
    facts: Value,
    game: Value,
    random: Value,
    random_index: usize,
    #[serde(default)]
    random_cursors: BTreeMap<String, usize>,
    schedule: turns::TurnSchedule,
    rng: u64,
    next_object: u64,
    next_event: u64,
    next_group: u64,
    next_decision: u64,
    draws_failed: BTreeSet<String>,
}

/// A cloneable game. Only this type resolves effects, creates prompts and hides private state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    catalog: Arc<Catalog>,
    state: State,
    #[serde(skip)]
    emitted: Vec<Value>,
    #[serde(skip)]
    node: String,
}

const ZONES: [&str; 14] = [
    "deck",
    "evolve_deck",
    "hand",
    "field",
    "ex",
    "cemetery",
    "banish",
    "resolution",
    "evolution",
    "race",
    "drive",
    "trigger",
    "equipment",
    "void",
];

pub(crate) fn string(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}
pub(crate) fn int(value: &Value) -> i64 {
    value.as_i64().unwrap_or_default()
}
pub(crate) fn list(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}
pub(crate) fn other(player: &str) -> &'static str {
    if player == "P1" { "P2" } else { "P1" }
}
pub(crate) fn scalar(value: &Value) -> i64 {
    value
        .as_i64()
        .or_else(|| {
            let v = value.as_str()?;
            v.parse().ok()
        })
        .unwrap_or_default()
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Rule domains share one private state and are split into focused modules."
)]
impl Game {
    /// Builds a neutral position without reading question identifiers or expected answers.
    ///
    /// # Errors
    /// Rejects missing card facts, malformed positions and unsupported historical state.
    #[expect(
        clippy::too_many_lines,
        reason = "All initial authoritative fields are assembled together for snapshot auditing."
    )]
    pub fn new(
        catalog: Arc<Catalog>,
        setup: &Value,
        facts: &Value,
        random: &Value,
        seed: &str,
    ) -> Result<Self> {
        let mut position = Self::prepare_opening(setup);
        if position["history"] == "none" {
            position["semantic_state"] = json!({});
        }
        if !list(&position["turn"]["extra_turns"]).is_empty() {
            return Err(EngineFailure::Unsupported(
                "legacy extra-turn state does not record the suspended normal turn".into(),
            ));
        }
        let mut players = BTreeMap::new();
        let mut objects = BTreeMap::new();
        for seat in ["P1", "P2"] {
            let entry = &position["players"][seat];
            let mut zones = BTreeMap::new();
            for zone in ZONES {
                let mut ids = Vec::new();
                for item in list(&entry["zones"][zone]) {
                    if item.get("filler").is_some() {
                        ids.push(item);
                        continue;
                    }
                    let id = string(&item["id"]).to_owned();
                    let card = string(&item["card"]).to_owned();
                    let owner = item["owner"].as_str().unwrap_or(seat);
                    if !matches!(owner, "P1" | "P2") {
                        return Err(invalid("object owner must be a player"));
                    }
                    let face_index =
                        usize::try_from(int(&item["state"]["face"])).map_err(invalid)?;
                    let face = catalog.face(&card, face_index)?;
                    let mut attrs = json!({"power":scalar(&face["power"]),"hp":scalar(&face["hp"]),"max_hp":scalar(&face["hp"]),"acted":false,"evolved":false,"entered_this_turn":false,"face":face_index,"damage":0_i64,"counters":{},"keywords":[],"silenced":false,"stats_increased_this_turn":false});
                    if let Some(patch) = item["state"].as_object() {
                        for (key, value) in patch {
                            attrs[key] = value.clone();
                        }
                    }
                    catalog.import_attributes(&mut attrs)?;
                    if item["state"].get("damage").is_some() && item["state"].get("hp").is_none() {
                        attrs["hp"] =
                            json!(int(&attrs["hp"]).saturating_sub(int(&attrs["damage"])));
                    }
                    if zone == "evolve_deck" {
                        attrs["face_up"] = json!(false);
                    }
                    if let Some(face_up) = item.get("face_up") {
                        attrs["face_up"] = face_up.clone();
                    }
                    if objects
                        .insert(
                            id.clone(),
                            Object {
                                id: id.clone(),
                                card,
                                owner: owner.into(),
                                controller: seat.into(),
                                zone: zone.into(),
                                generation: 0,
                                state: attrs,
                            },
                        )
                        .is_some()
                    {
                        return Err(invalid(format!("duplicate physical id: {id}")));
                    }
                    ids.push(json!(id));
                }
                zones.insert(zone.into(), ids);
            }
            players.insert(
                seat.into(),
                Player {
                    leader: entry["leader"].clone(),
                    pp: entry["pp"].clone(),
                    ep: int(&entry["ep"]),
                    sep: int(&entry["sep"]),
                    construction: string(&entry["construction"]).into(),
                    title: string(&entry["title"]).into(),
                    deck_list: entry["deck_list"].clone(),
                    zones,
                },
            );
        }
        let mut state = State {
            players,
            objects,
            turn: position["turn"].clone(),
            room: position["room"].clone(),
            pending: Vec::new(),
            frame: None,
            prompt: None,
            flow: json!({"kind":"main"}),
            counters: BTreeMap::new(),
            continuous: list(&position["semantic_state"]["continuous_effects"]),
            delayed: Vec::new(),
            used: BTreeMap::new(),
            event_occurrences: BTreeMap::new(),
            knowledge: BTreeMap::new(),
            facts: facts.clone(),
            game: json!({"ended":false}),
            random: random.clone(),
            random_index: 0,
            random_cursors: BTreeMap::new(),
            schedule: turns::TurnSchedule::load(&position["semantic_state"]["turn_schedule"])?,
            rng: seed.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |s, b| {
                (s ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
            }),
            next_object: 1,
            next_event: 1,
            next_group: 1,
            next_decision: 1,
            draws_failed: BTreeSet::new(),
        };
        for seat in ["P1", "P2"] {
            state.knowledge.insert(seat.into(), Knowledge::default());
            for counter in [
                "cards_played",
                "evolve_played",
                "evolutions",
                "leader_damaged",
                "leader_hp_decreased",
                "leader_hp_increased",
                "ub_activated",
                "discarded",
                "attacks",
            ] {
                state.counters.insert(format!("{seat}.{counter}"), 0);
            }
        }
        if let Some(counters) = position["semantic_state"]["counters_this_turn"].as_object() {
            for (key, value) in counters {
                if let Some(entries) = value.as_object() {
                    for (name, count) in entries {
                        if let Some(count) = count.as_i64() {
                            state.counters.insert(format!("{key}.{name}"), count);
                        }
                    }
                } else if let Some(count) = value.as_i64() {
                    state.counters.insert(key.clone(), count);
                } else {
                    return Err(invalid("turn counter must be an integer or player map"));
                }
            }
        }
        let mut game = Self {
            catalog,
            state,
            emitted: Vec::new(),
            node: "opening".into(),
        };
        game.initialize_evolved(&position)?;
        game.initialize_history(&position)?;
        #[expect(
            clippy::needless_collect,
            reason = "The snapshot releases the immutable state borrow before learning mutates knowledge."
        )]
        for object in game.state.objects.values().cloned().collect::<Vec<_>>() {
            for seat in ["P1", "P2"] {
                if object.zone != "void"
                    && (!matches!(object.zone.as_str(), "hand" | "deck" | "evolve_deck")
                        || (object.controller == seat && object.zone != "deck"))
                {
                    game.learn(seat, &object.id, true);
                }
            }
        }
        if setup["pregame"] == true {
            game.initialize_opening()?;
        }
        Ok(game)
    }

    fn initialize_evolved(&mut self, setup: &Value) -> Result<()> {
        let ids: Vec<_> = self.state.objects.keys().cloned().collect();
        for id in ids {
            let object = self.object(&id)?.clone();
            let evolved = string(&object.state["evolved_with"]);
            if evolved.is_empty() {
                continue;
            }
            let face = self.face(evolved)?.clone();
            let printed = self.catalog.face(&object.card, 0)?;
            let dp = scalar(&face["power"]).saturating_sub(scalar(&printed["power"]));
            let dh = scalar(&face["hp"]).saturating_sub(scalar(&printed["hp"]));
            let patch = list(&setup["players"][&object.controller]["zones"][&object.zone])
                .into_iter()
                .find(|value| value["id"] == id)
                .unwrap_or(Value::Null);
            let attrs = &mut self.object_mut(&id)?.state;
            if patch["state"].get("power").is_none() {
                attrs["power"] = json!(int(&attrs["power"]).saturating_add(dp));
            }
            if patch["state"].get("hp").is_none() {
                attrs["hp"] = json!(int(&attrs["hp"]).saturating_add(dh));
            }
            if patch["state"].get("max_hp").is_none() {
                attrs["max_hp"] = json!(int(&attrs["max_hp"]).saturating_add(dh));
            }
        }
        Ok(())
    }

    fn initialize_history(&mut self, setup: &Value) -> Result<()> {
        for object in self.state.objects.values_mut() {
            if object.state.get("attacks_this_turn").is_none()
                && let Some(count) = self.state.counters.get(&format!("{}.attacks", object.id))
            {
                object.state["attacks_this_turn"] = json!(count);
            }
        }
        self.import_cost_history()?;
        self.import_event_occurrences(&setup["semantic_state"])?;
        if let Some(used) = setup["semantic_state"].get("used_this_turn") {
            let entries: Vec<AbilityUse> = serde_json::from_value(used.clone()).map_err(invalid)?;
            for mut entry in entries {
                let source = string(&entry.ability["source"]);
                let code = self.ability(source, &entry.ability)?;
                let key = Self::usage_key(source, entry.generation, &code);
                entry.ability = self.reference(source, &code);
                if self.state.used.insert(key, entry).is_some() {
                    return Err(invalid("duplicate ability usage in history"));
                }
            }
        }
        for pending in list(&setup["semantic_state"]["pending_triggers"]) {
            let reference = &pending["ability"];
            let source = string(&reference["source"]).to_owned();
            let code = pending
                .get("program")
                .map_or_else(|| self.ability(&source, reference), |code| Ok(code.clone()))?;
            self.push_pending(Pending {
                controller: string(&pending["controller"]).into(),
                reference: reference.clone(),
                event: pending["event"].clone(),
                code,
                source,
                cause: json!({"rule":"fixture"}),
                retained: pending["event_discriminator"] == true,
                id: pending["id"].as_str().map(str::to_owned),
                context: pending
                    .get("context")
                    .map(|value| serde_json::from_value(value.clone()).map_err(invalid))
                    .transpose()?,
            });
        }
        if !list(&setup["semantic_state"]["delayed_triggers"]).is_empty() {
            return Err(EngineFailure::Unsupported(
                "neutral historical delayed text requires a typed history import".into(),
            ));
        }
        Ok(())
    }

    fn object(&self, id: &str) -> Result<&Object> {
        self.state
            .objects
            .get(id)
            .ok_or_else(|| invalid(format!("unknown object: {id}")))
    }
    fn object_mut(&mut self, id: &str) -> Result<&mut Object> {
        self.state
            .objects
            .get_mut(id)
            .ok_or_else(|| invalid(format!("unknown object: {id}")))
    }
    fn player(&self, seat: &str) -> Result<&Player> {
        self.state
            .players
            .get(seat)
            .ok_or_else(|| invalid(format!("unknown seat: {seat}")))
    }
    fn player_mut(&mut self, seat: &str) -> Result<&mut Player> {
        self.state
            .players
            .get_mut(seat)
            .ok_or_else(|| invalid(format!("unknown seat: {seat}")))
    }
    fn zone(&self, seat: &str, zone: &str) -> Vec<Value> {
        self.state
            .players
            .get(seat)
            .and_then(|p| p.zones.get(zone))
            .cloned()
            .unwrap_or_default()
    }
    fn zone_ids(&self, seat: &str, zone: &str) -> Vec<String> {
        self.zone(seat, zone)
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    }
    fn zone_count(&self, seat: &str, zone: &str) -> i64 {
        self.zone(seat, zone)
            .iter()
            .map(|v| v.get("filler").map_or(1, int))
            .sum()
    }
    fn face(&self, id: &str) -> Result<&Value> {
        let object = self.object(id)?;
        let number = object.state["evolved_with"]
            .as_str()
            .and_then(|evolved_id| self.state.objects.get(evolved_id))
            .map_or(object.card.as_str(), |card| card.card.as_str());
        self.catalog.face(
            number,
            usize::try_from(int(&object.state["face"])).map_err(invalid)?,
        )
    }
    fn object_type(&self, id: &str) -> Result<&str> {
        let object = self.object(id)?;
        Ok(object.state["card_type"]
            .as_str()
            .unwrap_or(string(&self.face(id)?["card_type"])))
    }
    fn printed_abilities(&self, id: &str) -> Result<Vec<Value>> {
        let object = self.object(id)?;
        if object.state["silenced"] == true {
            return Ok(Vec::new());
        }
        let number = object.state["evolved_with"]
            .as_str()
            .and_then(|evolved_id| self.state.objects.get(evolved_id))
            .map_or(object.card.as_str(), |card| card.card.as_str());
        Ok(list(&self.catalog.program(number)?["abilities"])
            .into_iter()
            .filter(|ability| {
                ability
                    .get("face")
                    .is_none_or(|face| int(face) == int(&object.state["face"]))
            })
            .flat_map(|ability| {
                if ability["kind"] == "static" && ability["body"]["op"] == "seq" {
                    list(&ability["body"]["steps"])
                        .into_iter()
                        .map(|body| {
                            let mut code = ability.clone();
                            code["body"] = body;
                            code
                        })
                        .collect::<Vec<_>>()
                } else {
                    vec![ability]
                }
            })
            .map(Self::resource_code)
            .collect())
    }
    fn abilities(&self, id: &str) -> Result<Vec<Value>> {
        let mut abilities = self.local_abilities(id)?;
        if self.object(id)?.zone == "field" {
            for source in self.field_ids() {
                for ability in self.local_abilities(&source)? {
                    let body = &ability["body"];
                    if body["op"] != "aura" || body.get("abilities").is_none() {
                        continue;
                    }
                    let frame = self.frame_for(&source)?;
                    if self.matches(id, &body["subjects"], &frame)?
                        && body
                            .get("condition")
                            .map_or(Ok(true), |condition| self.truth(condition, &frame))?
                    {
                        for mut granted in list(&body["abilities"])
                            .into_iter()
                            .map(Self::resource_code)
                        {
                            granted["granted_by"] = json!(source);
                            abilities.push(granted);
                        }
                    }
                }
            }
        }
        Ok(abilities)
    }
    fn object_attributes(&self, id: &str) -> Result<Value> {
        let object = self.object(id)?;
        let mut value = self.face(id)?.clone();
        if let Some(attrs) = object.state.as_object() {
            for (key, attr) in attrs {
                value[key] = attr.clone();
            }
        }
        value["id"] = json!(id);
        value["zone"] = json!(object.zone);
        value["cost"] = json!(scalar(&self.face(id)?["cost"]));
        value["token"] = json!(string(&self.face(id)?["card_type"]).contains("トークン"));
        value["generation"] = json!(object.generation);
        value["controller"] = json!(object.controller);
        Ok(value)
    }
    fn ability_sources(&self) -> Vec<String> {
        self.state
            .objects
            .values()
            .filter(|object| {
                object.zone == "field"
                    || self
                        .catalog
                        .programs
                        .get(&object.card)
                        .is_some_and(|program| {
                            list(&program["abilities"]).iter().any(|code| {
                                list(&code["active_zones"]).contains(&json!(object.zone))
                            })
                        })
            })
            .map(|object| object.id.clone())
            .collect()
    }
    fn ability(&self, id: &str, reference: &Value) -> Result<Value> {
        if let Some(code) = self
            .abilities(id)?
            .into_iter()
            .find(|code| legal::reference_matches(reference, &self.reference(id, code)))
        {
            return Ok(code);
        }
        if let Some(source) = reference["granted_by"].as_str() {
            return self
                .abilities(id)?
                .into_iter()
                .find(|ability| {
                    ability["granted_by"] == source && ability["line"] == reference["line"]
                })
                .ok_or_else(|| invalid("granted ability is no longer present"));
        }
        let number = reference["card"].as_str().unwrap_or(&self.object(id)?.card);
        list(&self.catalog.program(number)?["abilities"])
            .into_iter()
            .find(|a| {
                a["line"] == reference["line"]
                    && a.get("face").is_none_or(|face| {
                        *face
                            == reference.get("face").cloned().unwrap_or_else(|| {
                                self.state
                                    .objects
                                    .get(id)
                                    .map_or(Value::Null, |object| object.state["face"].clone())
                            })
                    })
                    && (reference["section"].is_null() || a["section"] == reference["section"])
                    && (reference["keyword"].is_null()
                        || a["keyword"] == reference["keyword"]
                        || self.catalog.keyword_name(string(&a["keyword"]))
                            == string(&reference["keyword"]))
            })
            .ok_or_else(|| EngineFailure::Unsupported(format!("ability not authored: {reference}")))
    }
    fn reference(&self, id: &str, ability: &Value) -> Value {
        if ability["granted_reference"].is_object() {
            let mut reference = ability["granted_reference"].clone();
            reference["source"] = json!(id);
            return reference;
        }
        let mut value = json!({"source":id,"line":ability["line"]});
        if let Some(object) = self.state.objects.get(id)
            && let Some(evolved) = object.state["evolved_with"]
                .as_str()
                .and_then(|evolved_id| self.state.objects.get(evolved_id))
        {
            value["card"] = json!(evolved.card);
        }
        if let Some(source) = ability["granted_by"].as_str()
            && let Some(object) = self.state.objects.get(source)
        {
            value["card"] = json!(object.card);
        }
        for key in ["section", "rule", "face"] {
            if let Some(v) = ability.get(key) {
                value[key] = v.clone();
            }
        }
        let shared_line = self.abilities(id).is_ok_and(|abilities| {
            abilities
                .iter()
                .filter(|other| {
                    other["line"] == ability["line"] && other["section"] == ability["section"]
                })
                .nth(1)
                .is_some()
        });
        if shared_line && let Some(keyword) = ability["keyword"].as_str() {
            value["keyword"] = json!(self.catalog.keyword_name(keyword));
        }
        value
    }
    fn active(&self) -> &str {
        string(&self.state.turn["active"])
    }
    fn frame_for(&self, id: &str) -> Result<Frame> {
        Ok(Frame {
            source: id.into(),
            controller: self.object(id)?.controller.clone(),
            ..Frame::default()
        })
    }
    fn bump(&mut self, key: &str, amount: i64) {
        let value = self.state.counters.entry(key.into()).or_default();
        *value = value.saturating_add(amount);
    }
    fn emit(&mut self, mut event: Value, cause: &Value, group: u64) -> String {
        let id = format!("{}:e{}", self.node, self.state.next_event);
        self.state.next_event = self.state.next_event.saturating_add(1);
        if event.get("by").is_none()
            && let Some(rule) = cause["rule"].as_str()
        {
            event["by"] = json!(format!("rule-{rule}"));
        }
        event["id"] = json!(id);
        event["cause"] = cause.clone();
        event["group"] = json!(group);
        self.emitted.push(event);
        id
    }
    const fn group(&mut self) -> u64 {
        let group = self.state.next_group;
        self.state.next_group = group.saturating_add(1);
        group
    }
    const fn random_word(&mut self) -> u64 {
        self.state.rng = self.state.rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut n = self.state.rng;
        n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        n ^ (n >> 31)
    }
}
