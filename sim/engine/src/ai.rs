//! Observation-only root sampling with an information-set-consistent rollout policy.

#![expect(
    clippy::indexing_slicing,
    reason = "Validated JSON uses total read indexing; writes target constructed objects."
)]
#![expect(
    clippy::float_arithmetic,
    reason = "Finite bounded heuristic scores are deliberately floating point, outside authoritative game state."
)]

use alloc::sync::Arc;
use core::cmp::Ordering;
use std::fs::read_to_string;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::catalog::{Catalog, yaml};
use crate::game::{Game, View, int, list, other, scalar, string};
use crate::{EngineFailure, Result, invalid};

/// Deck-specific evaluation, independent of card identifiers and position files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Stable profile key.
    pub id: String,
    /// Deck subtype.
    pub subtype: String,
    /// Period of the authored parameters.
    pub period: String,
    /// Value of leader life difference.
    pub life: f64,
    /// Value of field power and remaining health.
    pub board: f64,
    /// Value of a known hand card's printed strength.
    pub hand: f64,
    /// Additional value of initiative keywords.
    pub initiative: f64,
    /// Maximum independent worlds, before the edge budget is reached.
    pub samples: u32,
}

impl Profile {
    /// Reads a caller-selected data file; no profile can inspect hidden game state.
    ///
    /// # Errors
    /// Invalid YAML or non-finite/negative evaluation parameters.
    pub fn load(path: &Path) -> Result<Self> {
        let profile: Self = yaml(&read_to_string(path).map_err(invalid)?)?;
        if profile.id.is_empty()
            || profile.subtype.is_empty()
            || profile.period.is_empty()
            || profile.samples == 0
            || [
                profile.life,
                profile.board,
                profile.hand,
                profile.initiative,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0_f64)
        {
            return Err(invalid("invalid search profile"));
        }
        Ok(profile)
    }
}

/// Core-written audit, with one entry per applied player decision.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchLog {
    /// A forest rooted at independently sampled worlds.
    pub entries: Vec<Value>,
    /// Emitted rule events; reported separately from player decisions.
    pub engine_steps: u64,
}

/// Search result; the source match is never borrowed by this component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// Chosen root decision.
    pub decision: Value,
    /// Every root candidate and its sample-average score.
    pub candidates: Vec<(Value, f64)>,
    /// Bounded diagnostic input points, plus sample/frontier counts.
    pub trace: Vec<Value>,
    /// Wall time, outside deterministic decision and score fields.
    pub millis: u64,
    /// Core-written edge log.
    pub log: SearchLog,
}

/// Search accepts exactly the same packet that the seated client receives.
///
/// # Errors
/// Wrong decision maker, insufficient budget, unsupported belief constraints or invalid state.
#[expect(
    clippy::too_many_lines,
    reason = "Keeping the budget, common sample and score update in one loop makes the search audit explicit."
)]
pub fn think(
    catalog: &Arc<Catalog>,
    packet: &Value,
    seat: View,
    profile: &Profile,
    budget: u64,
    seed: &str,
) -> Result<Report> {
    let started = Instant::now();
    let player = seat
        .player()
        .ok_or_else(|| invalid("AI cannot use the referee view"))?;
    if packet["awaiting"]["by"] != player {
        return Err(invalid("AI is not the current decision maker"));
    }
    let candidates = list(&packet["legal"]);
    if candidates.is_empty() || budget < u64::try_from(candidates.len()).map_err(invalid)? {
        return Err(invalid("budget must cover each root action at least once"));
    }
    let mut scores = vec![(0.0_f64, 0_u32); candidates.len()];
    let mut log = SearchLog::default();
    let mut trace = Vec::new();
    let mut samples = 0_u32;
    let mut frontiers = 0_u32;
    'samples: for sample in 0..profile.samples {
        let world = Game::from_observation(
            Arc::clone(catalog),
            packet,
            player,
            &format!("{seed}:{sample}"),
        )?;
        let assumed = world.projection(View::P2)?;
        let sample_hand = list(&assumed["P2"]["hand"])
            .iter()
            .filter_map(Value::as_str)
            .map(|id| json!({"id":id,"card":assumed["objects"][id]["card"]}))
            .collect::<Vec<_>>();
        samples = samples.saturating_add(1);
        for (index, candidate) in candidates.iter().enumerate() {
            if u64::try_from(log.entries.len()).map_err(invalid)? >= budget {
                break 'samples;
            }
            let mut branch = world.clone();
            let mut action = candidate.clone();
            let mut parent = None;
            for depth in 0..128_u32 {
                let reserve = if sample == 0 && depth > 0 {
                    u64::try_from(candidates.len().saturating_sub(index.saturating_add(1)))
                        .map_err(invalid)?
                } else {
                    0
                };
                if u64::try_from(log.entries.len()).map_err(invalid)?
                    >= budget.saturating_sub(reserve)
                {
                    frontiers = frontiers.saturating_add(1);
                    break;
                }
                match branch.search_step(&action, &mut log, u64::from(sample), parent, &sample_hand)
                {
                    Ok(edge) => parent = Some(edge),
                    Err(EngineFailure::Unsupported(reason)) => {
                        frontiers = frontiers.saturating_add(1);
                        if trace.len() < 128 {
                            trace.push(json!({"frontier":"unsupported","reason":reason}));
                        }
                        break;
                    }
                    Err(error) => return Err(error),
                }
                let leaf = branch.projection(seat)?;
                if leaf["game"]["ended"] == true
                    || leaf["turn"]["active"] != packet["turn"]["active"]
                {
                    break;
                }
                let actor = match string(&leaf["awaiting"]["by"]) {
                    "P1" => View::P1,
                    "P2" => View::P2,
                    _ => return Err(invalid("sample has no next decision maker")),
                };
                let observation = branch.projection(actor)?;
                let options = list(&observation["legal"]);
                if options.is_empty() {
                    return Err(invalid("live sample has no legal decision"));
                }
                if trace.len() < 128 {
                    trace.push(json!({"depth":depth.saturating_add(1),"by":observation["awaiting"]["by"],"at":observation["awaiting"]["at"],"options":describe_options(&observation,&options)}));
                }
                action = rollout(catalog, &observation, profile, &options)?;
                if depth == 127 {
                    frontiers = frontiers.saturating_add(1);
                }
            }
            let leaf = branch.projection(seat)?;
            let value = evaluate(catalog, &leaf, player, profile)?;
            if let Some((total, count)) = scores.get_mut(index) {
                *total += value;
                *count = count.saturating_add(1);
            }
        }
    }
    let mut ranked = candidates
        .into_iter()
        .zip(scores)
        .map(|(candidate, (total, count))| {
            (
                candidate,
                if count == 0 {
                    f64::NEG_INFINITY
                } else {
                    total / f64::from(count)
                },
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|(a, sa), (b, sb)| {
        sb.partial_cmp(sa)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.to_string().cmp(&b.to_string()))
    });
    let decision = ranked
        .first()
        .map(|(action, _)| action.clone())
        .ok_or_else(|| invalid("empty result"))?;
    trace.push(json!({"samples":samples,"unfinished_frontiers":frontiers,"prior":"public-list-or-authored-class","policy":"observation-only-greedy","horizon":"current-turn"}));
    Ok(Report {
        decision,
        candidates: ranked,
        trace,
        millis: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        log,
    })
}

fn number(value: i64) -> f64 {
    f64::from(
        i32::try_from(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX))).unwrap_or_default(),
    )
}

fn describe_options(packet: &Value, options: &[Value]) -> Vec<Value> {
    options
        .iter()
        .map(|action| {
            let mut described = action.clone();
            if action["do"] == "play" {
                let id = string(&action["card"]);
                described["object"] = json!(id);
                described["card"] = packet["objects"][id]["card"].clone();
            }
            described
        })
        .collect()
}

fn strength(catalog: &Catalog, object: &Value, profile: &Profile, printed: bool) -> Result<f64> {
    let card = string(&object["card"]);
    let face = catalog.face(card, 0)?;
    let power = if printed {
        scalar(&face["power"])
    } else {
        int(&object["power"])
    };
    let hp = if printed {
        scalar(&face["hp"])
    } else {
        int(&object["hp"])
    };
    let mut value = number(power) + number(hp.max(0)) * 0.5_f64;
    let program = catalog.program(card)?;
    for ability in list(&program["abilities"]) {
        if ability["body"]["op"] == "keyword"
            && matches!(
                string(&ability["body"]["name"]),
                "storm" | "rush" | "bane" | "guard"
            )
        {
            value += profile.initiative;
        }
    }
    if string(&face["card_type"]).contains("スペル") {
        value += number(scalar(&face["cost"])).max(1.0_f64);
    }
    Ok(value)
}

fn evaluate(catalog: &Catalog, packet: &Value, player: &str, profile: &Profile) -> Result<f64> {
    let opponent = other(player);
    if packet["game"]["ended"] == true {
        return Ok(if packet["game"]["winner"].is_null() {
            0.0_f64
        } else if packet["game"]["winner"] == player {
            100_000.0_f64
        } else {
            -100_000.0_f64
        });
    }
    let mut value = profile.life
        * number(
            int(&packet[player]["leader"]["life"])
                .saturating_sub(int(&packet[opponent]["leader"]["life"])),
        );
    for (seat, sign) in [(player, 1.0_f64), (opponent, -1.0_f64)] {
        for id in list(&packet[seat]["field"])
            .iter()
            .filter_map(Value::as_str)
        {
            value = (sign * profile.board).mul_add(
                strength(catalog, &packet["objects"][id], profile, false)?,
                value,
            );
        }
        for item in list(&packet[seat]["hand"]) {
            value = (sign * profile.hand).mul_add(
                if let Some(id) = item.as_str() {
                    strength(catalog, &packet["objects"][id], profile, true)?
                } else {
                    number(int(&item["filler"])) * 3.0_f64
                },
                value,
            );
        }
    }
    Ok(value)
}

// Only an acting player's packet reaches this policy; its future choice is shared by indistinguishable worlds.
fn rollout(
    catalog: &Catalog,
    packet: &Value,
    profile: &Profile,
    options: &[Value],
) -> Result<Value> {
    let mut ranked = options
        .iter()
        .map(|action| Ok((action, action_value(catalog, packet, profile, action)?)))
        .collect::<Result<Vec<_>>>()?;
    ranked.sort_by(|(a, sa), (b, sb)| {
        sb.partial_cmp(sa)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.to_string().cmp(&b.to_string()))
    });
    ranked
        .first()
        .map(|(a, _)| (*a).clone())
        .ok_or_else(|| invalid("rollout choices empty"))
}

fn action_value(
    catalog: &Catalog,
    packet: &Value,
    profile: &Profile,
    action: &Value,
) -> Result<f64> {
    let player = string(&packet["awaiting"]["by"]);
    let value = match string(&action["do"]) {
        "attack" => {
            let attacker = &packet["objects"][string(&action["attacker"])];
            let power = int(&attacker["power"]);
            if string(&action["target"]).ends_with(".leader") {
                if power >= int(&packet[other(player)]["leader"]["life"]) {
                    100_000.0_f64
                } else {
                    number(power) * profile.life
                }
            } else {
                let target = &packet["objects"][string(&action["target"])];
                let killed = power >= int(&target["hp"]);
                let dies = int(&target["power"]) >= int(&attacker["hp"]);
                profile.board.mul_add(
                    if killed {
                        strength(catalog, target, profile, false)?
                    } else {
                        number(power) * 0.5_f64
                    },
                    -(if dies {
                        profile.board * strength(catalog, attacker, profile, false)?
                    } else {
                        profile.board * number(int(&target["power"])) * 0.5_f64
                    }),
                )
            }
        }
        "play" => {
            let object = &packet["objects"][string(&action["card"])];
            let face = catalog.face(string(&object["card"]), 0)?;
            if string(&face["card_type"]).contains("スペル") {
                let mut damage = 0_i64;
                for ability in list(&catalog.program(string(&object["card"]))?["abilities"]) {
                    damage = damage.max(max_damage(&ability["body"]));
                }
                let mut benefit = 0.0_f64;
                for targets in action["targets"]
                    .as_object()
                    .into_iter()
                    .flat_map(|m| m.values())
                {
                    for id in list(targets).iter().filter_map(Value::as_str) {
                        if id.ends_with(".leader") {
                            benefit = number(damage).mul_add(profile.life, benefit);
                        } else {
                            let target = &packet["objects"][id];
                            benefit += if damage >= int(&target["hp"]) {
                                profile
                                    .board
                                    .mul_add(strength(catalog, target, profile, false)?, 2.0_f64)
                            } else {
                                profile.board * number(damage) * 0.5_f64
                            };
                            if packet["flow"]["attacker"] == id {
                                benefit =
                                    number(int(&target["power"])).mul_add(profile.life, benefit);
                            }
                        }
                    }
                }
                benefit + 0.1_f64
            } else {
                profile
                    .board
                    .mul_add(strength(catalog, object, profile, true)?, 1.0_f64)
            }
        }
        "resolve-choice" | "guard-act" => {
            let mut value = 0.0_f64;
            for id in list(&action["select"]).iter().filter_map(Value::as_str) {
                value += strength(catalog, &packet["objects"][id], profile, true)?;
            }
            if action["choice"] == "execute" {
                value += 1.0_f64;
            }
            value
        }
        "place-acted" => {
            if action["acted"] == true {
                1.0_f64
            } else {
                0.0_f64
            }
        }
        "choose-pending" | "activate" | "evolve" => 1.0_f64,
        "end-phase" | "pass" => -0.1_f64,
        _ => 0.0_f64,
    };
    Ok(value)
}

fn max_damage(value: &Value) -> i64 {
    match value {
        Value::Object(fields) => {
            if value["op"] == "damage" {
                int(&value["amount"])
            } else {
                fields.values().map(max_damage).max().unwrap_or_default()
            }
        }
        Value::Array(values) => values.iter().map(max_damage).max().unwrap_or_default(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => 0,
    }
}
