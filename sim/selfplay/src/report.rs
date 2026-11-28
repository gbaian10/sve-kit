//! Running many seeds and summarising what they found.

use crate::arena::Arena;
use crate::play::{End, Limits, Match, StepContext, play};
use crate::usage::Uses;
use alloc::collections::{BTreeMap, BTreeSet};
use core::cmp::Reverse;
use core::sync::atomic::{AtomicU64, Ordering};
use serde_json::{Value, json};
use std::sync::mpsc::channel;
use std::thread;
use sve_battle_contract::Seat;

const SEEDS_KEPT: usize = 10;
const SLOW_KEPT: usize = 20;

/// Play every seed of `first..first + games` on `jobs` threads, handing each finished match
/// to `sink` on the calling thread in completion order.
pub fn run_seeds<F>(arena: &Arena, first: u64, games: u64, jobs: usize, limits: Limits, mut sink: F)
where
    F: FnMut(Match),
{
    let next = AtomicU64::new(0);
    let (sender, receiver) = channel();
    thread::scope(|scope| {
        for _ in 0..jobs.max(1) {
            let sender = sender.clone();
            let next = &next;
            scope.spawn(move || {
                loop {
                    let offset = next.fetch_add(1, Ordering::Relaxed);
                    if offset >= games {
                        break;
                    }
                    let played = play(arena, first.saturating_add(offset), limits);
                    if sender.send(played).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
        for played in receiver {
            sink(played);
        }
    });
}

#[derive(Debug, Default)]
struct Group {
    count: u64,
    seeds: Vec<u64>,
    cards: BTreeMap<String, u64>,
    example: String,
}

/// Totals over a run.
#[derive(Debug, Default)]
pub struct Report {
    games: u64,
    outcomes: BTreeMap<String, u64>,
    failures: BTreeMap<(String, String), Group>,
    receipts: BTreeMap<String, u64>,
    engine_micros: Vec<u64>,
    encode_micros: Vec<u64>,
    step_events: Vec<u64>,
    match_micros: Vec<u64>,
    match_decisions: Vec<u64>,
    match_events: Vec<u64>,
    match_turns: Vec<u64>,
    slow: Vec<(u64, StepContext)>,
    built: BTreeMap<String, u64>,
    seen: BTreeMap<String, u64>,
    uses: BTreeMap<String, Uses>,
    failed: BTreeMap<String, u64>,
    listed: BTreeMap<usize, Seated>,
}

/// Results of one listed deck, counted per seat it took.
#[derive(Debug, Default)]
struct Seated {
    seats: u64,
    wins: u64,
    losses: u64,
    draws: u64,
    failures: u64,
    mirrors: u64,
}

const fn bump(count: &mut u64) {
    *count = count.saturating_add(1);
}

fn deck_card_number<'text>(arena: &'text Arena, reference: &'text str) -> &'text str {
    let printing = arena
        .printings
        .get(reference)
        .map_or(reference, String::as_str);
    arena.number(printing)
}

/// Digits stripped, so one diagnostic about different objects groups together.
fn signature(detail: &str) -> String {
    let mut text = String::new();
    let mut digits = false;
    for letter in detail.chars().take(200) {
        if letter.is_ascii_digit() {
            if !digits {
                text.push('#');
            }
            digits = true;
        } else {
            text.push(letter);
            digits = false;
        }
    }
    text
}

fn percentile(sorted: &[u64], percent: usize) -> u64 {
    let last = sorted.len().saturating_sub(1);
    sorted
        .get(last.saturating_mul(percent).checked_div(100).unwrap_or(0))
        .copied()
        .unwrap_or(0)
}

fn spread(values: &[u64]) -> Value {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    json!({
        "count": sorted.len(),
        "total": sorted.iter().fold(0_u64, |sum, value| sum.saturating_add(*value)),
        "p50": percentile(&sorted, 50),
        "p95": percentile(&sorted, 95),
        "p99": percentile(&sorted, 99),
        "max": sorted.last().copied().unwrap_or(0),
    })
}

impl Report {
    /// Add one match.
    pub fn add(&mut self, arena: &Arena, played: &Match) {
        self.games = self.games.saturating_add(1);
        let outcome = match &played.end {
            End::Finished { winner } => winner.map_or_else(
                || "draw".to_owned(),
                |seat| {
                    serde_json::to_value(seat)
                        .ok()
                        .and_then(|name| name.as_str().map(str::to_owned))
                        .unwrap_or_default()
                },
            ),
            End::Failed(failure) => {
                let group = self
                    .failures
                    .entry((failure.kind.to_owned(), signature(&failure.detail)))
                    .or_default();
                group.count = group.count.saturating_add(1);
                if group.seeds.len() < SEEDS_KEPT {
                    group.seeds.push(played.record.seed);
                }
                if group.example.is_empty() {
                    group.example.clone_from(&failure.detail);
                }
                for card in &failure.cards {
                    let uses = group.cards.entry(card.clone()).or_default();
                    *uses = uses.saturating_add(1);
                }
                failure.kind.to_owned()
            }
        };
        let total = self.outcomes.entry(outcome).or_default();
        *total = total.saturating_add(1);
        for step in &played.record.steps {
            let name = serde_json::to_value(step.result)
                .ok()
                .and_then(|name| name.as_str().map(str::to_owned))
                .unwrap_or_default();
            let uses = self.receipts.entry(name).or_default();
            *uses = uses.saturating_add(1);
        }
        let mut engine = 0_u64;
        let mut events = 0_u64;
        for cost in &played.costs {
            engine = engine.saturating_add(cost.engine_micros);
            events = events.saturating_add(u64::try_from(cost.events).unwrap_or(u64::MAX));
            self.engine_micros.push(cost.engine_micros);
            self.encode_micros.push(cost.encode_micros);
            self.step_events
                .push(u64::try_from(cost.events).unwrap_or(u64::MAX));
        }
        self.match_micros.push(engine);
        self.match_events.push(events);
        self.match_decisions
            .push(u64::try_from(played.record.steps.len()).unwrap_or(u64::MAX));
        self.match_turns.push(played.turns);
        self.slow.extend(
            played
                .slow
                .iter()
                .map(|step| (played.record.seed, step.clone())),
        );
        self.slow
            .sort_by_key(|(_, step)| Reverse(step.cost.engine_micros));
        self.slow.truncate(SLOW_KEPT);
        let failed = matches!(played.end, End::Failed(_));
        let mut either = BTreeSet::new();
        for deck in &played.record.setup.decks {
            let numbers = deck
                .main
                .iter()
                .chain(&deck.evolve)
                .map(|reference| deck_card_number(arena, reference));
            either.extend(numbers);
        }
        for number in either {
            bump(self.built.entry(number.to_owned()).or_default());
            if failed {
                bump(self.failed.entry(number.to_owned()).or_default());
            }
        }
        for number in &played.seen {
            bump(self.seen.entry(number.clone()).or_default());
        }
        for (number, uses) in &played.uses {
            self.uses.entry(number.clone()).or_default().add(uses);
        }
        if let Some(positions) = played.listed {
            self.seat(positions, &played.end);
        }
    }

    fn seat(&mut self, positions: [usize; 2], end: &End) {
        for (seat, position) in [Seat::P1, Seat::P2].into_iter().zip(positions) {
            let entry = self.listed.entry(position).or_default();
            bump(&mut entry.seats);
            match end {
                End::Finished { winner: None } => bump(&mut entry.draws),
                End::Finished {
                    winner: Some(winner),
                } if *winner == seat => bump(&mut entry.wins),
                End::Finished { .. } => bump(&mut entry.losses),
                End::Failed(_) => bump(&mut entry.failures),
            }
        }
        let [first, second] = positions;
        if first == second {
            bump(&mut self.listed.entry(first).or_default().mirrors);
        }
    }

    /// Each observed card number, its event counts and the matches where it was in a deck
    /// or publicly visible, including failed matches whose decks held it.
    #[must_use]
    pub fn cards(&self) -> Value {
        let numbers: BTreeSet<&String> = self
            .built
            .keys()
            .chain(self.seen.keys())
            .chain(self.uses.keys())
            .collect();
        let count =
            |map: &BTreeMap<String, u64>, number: &String| map.get(number).copied().unwrap_or(0);
        numbers
            .into_iter()
            .map(|number| {
                let mut entry = json!(self.uses.get(number).copied().unwrap_or_default());
                if let Some(fields) = entry.as_object_mut() {
                    fields.insert("built".into(), json!(count(&self.built, number)));
                    fields.insert("seen".into(), json!(count(&self.seen, number)));
                    fields.insert("failed".into(), json!(count(&self.failed, number)));
                }
                (number.clone(), entry)
            })
            .collect::<serde_json::Map<_, _>>()
            .into()
    }

    /// The whole run as one document.
    #[must_use]
    pub fn summary(&self, arena: &Arena) -> Value {
        let eligible = arena.lineup.as_ref().map_or_else(
            || arena.pools.eligible(),
            |lineup| {
                lineup
                    .card_references()
                    .map(|reference| deck_card_number(arena, reference))
                    .collect()
            },
        );
        let lineup: Vec<Value> = arena.lineup.as_ref().map_or_else(Vec::new, |lineup| {
            (0..lineup.len())
                .map(|position| {
                    let empty = Seated::default();
                    let seated = self.listed.get(&position).unwrap_or(&empty);
                    json!({
                        "label": lineup.label(position),
                        "seats": seated.seats,
                        "wins": seated.wins,
                        "losses": seated.losses,
                        "draws": seated.draws,
                        "failures": seated.failures,
                        "mirrors": seated.mirrors,
                    })
                })
                .collect()
        });
        let failures: Vec<Value> = self
            .failures
            .iter()
            .map(|((kind, signature), group)| {
                let mut cards: Vec<(&String, &u64)> = group.cards.iter().collect();
                cards.sort_by_key(|(_, uses)| Reverse(**uses));
                json!({
                    "kind": kind,
                    "signature": signature,
                    "count": group.count,
                    "seeds": group.seeds,
                    "example": group.example,
                    "cards": cards.into_iter().take(12).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({
            "games": self.games,
            "outcomes": self.outcomes,
            "failures": failures,
            "receipts": self.receipts,
            "step_engine_micros": spread(&self.engine_micros),
            "step_encode_micros": spread(&self.encode_micros),
            "step_events": spread(&self.step_events),
            "match_engine_micros": spread(&self.match_micros),
            "match_decisions": spread(&self.match_decisions),
            "match_events": spread(&self.match_events),
            "match_turns": spread(&self.match_turns),
            "lineup": lineup,
            "slowest_steps": self.slow.iter().map(|(seed, step)| json!({"seed": seed, "step": step})).collect::<Vec<_>>(),
            "cards": {
                "eligible": eligible.len(),
                "built": self.built.len(),
                "seen_public": self.seen.len(),
                "never_built": eligible.iter().filter(|number| !self.built.contains_key(**number)).collect::<Vec<_>>(),
                "never_seen_public": eligible.iter().filter(|number| !self.seen.contains_key(**number)).collect::<Vec<_>>(),
            },
        })
    }
}
