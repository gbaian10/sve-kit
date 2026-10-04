#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;

use serde_json::{Value, json};

use super::{Game, ZONES, int, list, string};
use crate::ai::SearchLog;
use crate::catalog::Catalog;
use crate::random::{Random, RandomAlgorithm};
use crate::{EngineFailure, Result, invalid};

impl Game {
    /// Samples a hypothetical world from one outgoing player packet, never from server state.
    ///
    /// # Errors
    /// Inconsistent public deck counts or a continuation unavailable to this observer.
    pub fn from_observation(
        catalog: Arc<Catalog>,
        packet: &Value,
        seat: &str,
        seed: &str,
    ) -> Result<Self> {
        Self::from_observation_with_random_algorithm(
            catalog,
            packet,
            seat,
            seed,
            RandomAlgorithm::ChaCha12V1,
        )
    }

    /// Reconstructs a hypothetical world with an explicitly selected seed algorithm.
    ///
    /// # Errors
    /// Inconsistent public deck counts or a continuation unavailable to this observer.
    pub fn from_observation_with_random_algorithm(
        catalog: Arc<Catalog>,
        packet: &Value,
        seat: &str,
        seed: &str,
        algorithm: RandomAlgorithm,
    ) -> Result<Self> {
        let mut random = Random::new(seed, algorithm);
        let mut setup = json!({"turn":packet["turn"],"room":packet["room"],"semantic_state":packet["semantic_state"],"players":{}});
        setup["semantic_state"]["delayed_triggers"] = json!([]);
        for owner in ["P1", "P2"] {
            let mut player = packet[owner].clone();
            player["zones"] = json!({});
            let mut pool = remaining_pool(&catalog, packet, owner, &mut random)?;
            for zone in ZONES {
                let mut objects = Vec::new();
                for item in list(&packet[owner][zone]) {
                    if let Some(id) = item.as_str() {
                        let visible = &packet["objects"][id];
                        objects.push(json!({"id":id,"card":visible["card"],"state":visible,"owner":visible_owner(packet,id,owner)}));
                    } else {
                        if zone != "hand" && zone != "deck" && int(&item["filler"]) > 0 {
                            return Err(EngineFailure::Unsupported(format!(
                                "unknown {zone} needs a separate belief prior"
                            )));
                        }
                        for _ in 0..int(&item["filler"]) {
                            let id = format!("belief-{owner}-{zone}-{}", objects.len());
                            let card = pool.pop().ok_or_else(|| {
                                EngineFailure::Unsupported(format!(
                                    "belief prior exhausted for {owner}/{zone}"
                                ))
                            })?;
                            objects.push(json!({"id":id,"card":card}));
                        }
                    }
                }
                player["zones"][zone] = json!(objects);
            }
            setup["players"][owner] = player;
        }
        let mut game = Self::new_with_random_algorithm(
            catalog,
            &setup,
            &Value::Null,
            &Value::Null,
            seed,
            algorithm,
        )?;
        game.state.delayed = list(&packet["semantic_state"]["delayed_triggers"]);
        for (id, object) in &mut game.state.objects {
            if let Some(visible) = packet["objects"].get(id) {
                object.state = visible.clone();
                game.catalog.import_attributes(&mut object.state)?;
                object.generation = visible["generation"].as_u64().unwrap_or_default();
            }
        }
        game.state.flow = packet["flow"].clone();
        game.state.game = packet["game"].clone();
        if packet.get("continuation").is_none() && packet["awaiting"]["at"] == "resolve" {
            return Err(EngineFailure::Unsupported(
                "observer has no inspectable resolution continuation".into(),
            ));
        }
        if let Some(continuation) = packet.get("continuation") {
            game.state.frame =
                serde_json::from_value(continuation["frame"].clone()).map_err(invalid)?;
            game.state.prompt =
                serde_json::from_value(continuation["prompt"].clone()).map_err(invalid)?;
        }

        if let Some(knowledge) = game.state.knowledge.get_mut(seat) {
            for known in list(&packet["known_cards"]) {
                knowledge.seen.insert(string(&known["id"]).into(), known);
            }
            knowledge.located.extend(
                list(&packet["knowledge"]["located"])
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
            knowledge.carried = list(&packet["knowledge"]["carried"]);
        }
        Ok(game)
    }

    /// Applies and records exactly one successful hypothetical player-decision edge.
    ///
    /// # Errors
    /// The sampled world rejects the decision or needs unsupported semantics.
    pub fn search_step(
        &mut self,
        decision: &Value,
        log: &mut SearchLog,
        sample: u64,
        parent: Option<u64>,
        sample_hand: &[Value],
    ) -> Result<u64> {
        let point = self.input_point();
        let edge = u64::try_from(log.entries.len())
            .map_err(invalid)?
            .saturating_add(1);
        let mut request = decision.clone();
        request["by"] = point["by"].clone();
        request["at"] = point["at"].clone();
        let rule = if point["at"] == "quick" {
            if self.state.flow["kind"] == "battle" {
                "8.4.7"
            } else {
                "7.4.5"
            }
        } else {
            "10.5.2"
        };
        let step = self.decide(&request, &format!("search-{sample}-{edge}"))?;
        if step.outcome.starts_with("cannot-") {
            return Err(invalid(format!("enumerated action rejected: {request}")));
        }
        let mut entry = json!({"edge":edge,"parent":parent,"sample":sample,"by":point["by"],"at":point["at"],"rule":rule,"decision":decision});
        if parent.is_none() {
            entry["sample_hand"] = json!(sample_hand);
        }
        log.entries.push(entry);
        log.engine_steps = log
            .engine_steps
            .saturating_add(u64::try_from(step.events.len()).map_err(invalid)?);
        Ok(edge)
    }
}

fn remaining_pool(
    catalog: &Catalog,
    packet: &Value,
    owner: &str,
    random: &mut Random,
) -> Result<Vec<String>> {
    let open = packet[owner]["deck_list"].as_array();
    let mut counts = BTreeMap::<String, i64>::new();
    if let Some(entries) = open {
        for entry in entries {
            counts.insert(string(&entry["card"]).into(), int(&entry["count"]));
        }
    } else {
        let class = string(&packet[owner]["leader"]["class"]);
        let singleton = list(&packet["known_cards"])
            .iter()
            .filter(|k| k["owner"] == owner)
            .any(|k| {
                catalog.program(string(&k["card"])).is_ok_and(|p| {
                    list(&p["abilities"])
                        .iter()
                        .any(|a| a["body"]["op"] == "deck_limit")
                })
            });
        let mut names = BTreeSet::new();
        for (number, program) in &catalog.programs {
            let face = catalog.face(number, 0)?;
            if program["status"] != "complete"
                || !supported_prior(program)
                || string(&face["card_type"]).contains("エボルヴ")
                || string(&face["card_type"]).contains("トークン")
            {
                continue;
            }
            let eligible = if packet[owner]["construction"] == "title" {
                face["title"] == packet[owner]["title"]
            } else {
                face["card_class"] == class || face["card_class"] == "ニュートラル"
            };
            if eligible && names.insert(string(&face["name"]).to_owned()) {
                counts.insert(number.clone(), if singleton { 1 } else { 3 });
            }
        }
    }
    let mut seen = BTreeSet::new();
    for controller in ["P1", "P2"] {
        for zone in ZONES {
            if matches!(zone, "evolve_deck" | "evolution" | "void") {
                continue;
            }
            for item in list(&packet[controller][zone]) {
                if let Some(id) = item.as_str() {
                    seen.insert(id.to_owned());
                    let card = string(&packet["objects"][id]["card"]);
                    if visible_owner(packet, id, controller) == owner
                        && let Some(count) = counts.get_mut(card)
                    {
                        *count = count.saturating_sub(1);
                    }
                }
            }
        }
    }
    // Known but unlocated identities constrain composition, never a hidden position.
    let hidden_count: usize = ["hand", "deck"]
        .iter()
        .map(|zone| {
            list(&packet[owner][*zone])
                .iter()
                .filter_map(|v| v["filler"].as_u64())
                .sum::<u64>()
        })
        .sum::<u64>()
        .try_into()
        .map_err(invalid)?;
    let mut forced = Vec::new();
    for known in list(&packet["known_cards"]) {
        let id = string(&known["id"]);
        if !seen.contains(id) && known["owner"] == owner && forced.len() < hidden_count {
            let card = string(&known["card"]);
            if let Some(count) = counts.get_mut(card)
                && *count > 0
            {
                *count = count.saturating_sub(1);
                forced.push(card.to_owned());
            }
        }
    }
    let mut pool = Vec::new();
    for (number, count) in counts {
        if count < 0 {
            return Err(invalid("visible cards exceed the public deck list"));
        }
        for _ in 0..count {
            pool.push(number.clone());
        }
    }
    if open.is_some() && pool.len().saturating_add(forced.len()) != hidden_count {
        return Err(invalid("public deck list and unknown zone counts disagree"));
    }
    random.shuffle(&mut pool);
    pool.truncate(hidden_count.saturating_sub(forced.len()));
    pool.extend(forced);
    random.shuffle(&mut pool);
    Ok(pool)
}

fn visible_owner<'packet>(
    packet: &'packet Value,
    id: &str,
    controller: &'packet str,
) -> &'packet str {
    packet["known_cards"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["id"] == id))
        .and_then(|entry| entry["owner"].as_str())
        .unwrap_or(controller)
}

fn supported_prior(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().all(supported_prior),
        Value::Object(fields) => {
            if Game::check_execution_parameters(value).is_err()
                || ["additional_costs", "advance", "limit_at", "active_zones"]
                    .iter()
                    .any(|key| fields.contains_key(*key))
            {
                return false;
            }
            if let Some(op) = fields.get("op").and_then(Value::as_str)
                && !matches!(
                    op,
                    "seq"
                        | "if"
                        | "per"
                        | "repeat"
                        | "for_each"
                        | "optional"
                        | "pay"
                        | "if_done"
                        | "choice"
                        | "select"
                        | "damage"
                        | "move"
                        | "destroy"
                        | "banish"
                        | "discard"
                        | "act"
                        | "stand"
                        | "draw"
                        | "look"
                        | "search"
                        | "shuffle"
                        | "reveal"
                        | "modify"
                        | "pp"
                        | "recover_pp"
                        | "delay"
                        | "keyword"
                        | "aura"
                        | "replace_damage"
                        | "adjust_cost"
                        | "restrict"
                        | "evolve"
                        | "deck_limit"
                )
            {
                return false;
            }
            fields.values().all(supported_prior)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => true,
    }
}
