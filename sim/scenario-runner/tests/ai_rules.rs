//! Hand-written rules for the few cards of H2, Q1b and R2, independent of the
//! positions' `legal_exact` tables: legal sets, outcomes and input points come from the
//! state. The positions' expectations must agree with it.

#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::absolute_paths,
    clippy::std_instead_of_alloc,
    clippy::arithmetic_side_effects,
    clippy::default_numeric_fallback,
    clippy::struct_field_names,
    reason = "test code: a tiny scripted rules engine, and a panic is a test failure"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Value, json};
use sve_scenario_runner::ai::{AiEngine, AiOutcome, AiReport, check_ai, load_positions};
use sve_scenario_runner::{EngineError, Fixture, Step, View};

const SHIKIGAMI: &str = "BP06-042";
const MONTEI: &str = "BP06-048";
const SNIPE: &str = "BP01-179";
const CUTTHROAT: &str = "BP19-SL26";

fn hp(card: &str) -> i64 {
    match card {
        MONTEI | CUTTHROAT => 1,
        "BP01-042" => 2,
        _ => 3,
    }
}

#[derive(Default, Clone)]
struct Card {
    id: String,
    card: String,
    damage: i64,
}

#[derive(Default)]
struct Rules {
    zones: BTreeMap<(String, String), Vec<Card>>,
    pp: BTreeMap<String, u64>,
    shuffles: Vec<Value>,
    /// Pending abilities by source id, with their controller.
    pending: Vec<(String, String)>,
    /// `{by, at}`; `None` at game end.
    point: Option<(String, String)>,
    /// The ability being resolved and what the player may choose from.
    resolving: Option<(String, Vec<Value>)>,
}

fn err(text: &str) -> EngineError {
    EngineError::Adapter(text.to_owned())
}

impl Rules {
    fn zone(&mut self, player: &str, zone: &str) -> &mut Vec<Card> {
        self.zones
            .entry((player.to_owned(), zone.to_owned()))
            .or_default()
    }

    fn take(&mut self, player: &str, zone: &str, id: &str) -> Result<Card, EngineError> {
        let list = self.zone(player, zone);
        let at = list
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| err(id))?;
        Ok(list.remove(at))
    }

    /// Applies the next controlled shuffle; it must be an ordering of the current deck.
    fn shuffle(&mut self, player: &str) -> Result<(), EngineError> {
        if self.shuffles.is_empty() {
            return Err(err("a shuffle happened without a controlled result"));
        }
        let next = self.shuffles.remove(0);
        let order: Vec<String> = next["result"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let deck = self.zone(player, "deck").clone();
        let mut have: Vec<&str> = deck.iter().map(|c| c.id.as_str()).collect();
        let mut want: Vec<&str> = order.iter().map(String::as_str).collect();
        have.sort_unstable();
        want.sort_unstable();
        if have != want {
            return Err(err("the controlled shuffle is not an ordering of the deck"));
        }
        let by_id: BTreeMap<String, Card> = deck.into_iter().map(|c| (c.id.clone(), c)).collect();
        *self.zone(player, "deck") = order.iter().map(|id| by_id[id].clone()).collect();
        Ok(())
    }

    fn after_resolution(&mut self) -> &'static str {
        self.resolving = None;
        self.point = Some(match self.pending.first() {
            Some((_, controller)) => (controller.clone(), "check-timing".to_owned()),
            None => ("P1".to_owned(), "main".to_owned()),
        });
        "resolved"
    }

    fn apply(&mut self, d: &Value) -> Result<&'static str, EngineError> {
        let what = d["do"].as_str().unwrap_or_default();
        match what {
            "play" if d["by"] == "P1" => self.play(d),
            "attack" => {
                self.point = Some(("P2".to_owned(), "quick".to_owned()));
                Ok("resolved")
            }
            "choose-pending" => {
                let source = d["pending"]["ability"]["source"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                let at = self
                    .pending
                    .iter()
                    .position(|(s, _)| *s == source)
                    .ok_or_else(|| err("no such pending ability"))?;
                let (_, controller) = self.pending.remove(at);
                let deck = self.zone(&controller, "deck").clone();
                let options = if source.starts_with('a') {
                    // A CUTTHROAT source (id prefix a) selects one card from the top eight.
                    deck.iter().take(8).map(|c| json!([c.id])).collect()
                } else {
                    // MONTEI searches the deck for SHIKIGAMI or chooses to find none (Q816).
                    let mut o: Vec<Value> = deck
                        .iter()
                        .filter(|c| c.card == SHIKIGAMI)
                        .map(|c| json!([c.id]))
                        .collect();
                    o.push(json!([]));
                    o
                };
                self.resolving = Some((controller.clone(), options));
                self.point = Some((controller, "resolve".to_owned()));
                Ok("paused")
            }
            "resolve-choice" => {
                let (player, options) = self
                    .resolving
                    .clone()
                    .ok_or_else(|| err("nothing to resolve"))?;
                if !options.contains(&d["select"]) {
                    return Ok("cannot-play");
                }
                for id in d["select"].as_array().unwrap() {
                    let card = self.take(&player, "deck", id.as_str().unwrap())?;
                    self.zone(&player, "hand").push(card);
                }
                self.shuffle(&player)?;
                Ok(self.after_resolution())
            }
            _ => Err(EngineError::Unsupported(format!("{d}"))),
        }
    }

    fn play(&mut self, d: &Value) -> Result<&'static str, EngineError> {
        let id = d["card"].as_str().unwrap();
        let card = self.take("P1", "hand", id)?;
        if card.card == CUTTHROAT {
            *self.pp.get_mut("P1").unwrap() -= 1;
            self.zone("P1", "field").push(card);
            self.pending.push((id.to_owned(), "P1".to_owned()));
            self.point = Some(("P1".to_owned(), "check-timing".to_owned()));
            return Ok("resolved");
        }
        // Damage resolves simultaneously; check destruction only after applying the whole distribution.
        for (target, amount) in d["distribute"]["1"].as_object().into_iter().flatten() {
            let field = self.zone("P2", "field");
            let hit = field.iter_mut().find(|c| &c.id == target).unwrap();
            hit.damage += amount.as_i64().unwrap();
        }
        let field = core::mem::take(self.zone("P2", "field"));
        for follower in field {
            if follower.damage >= hp(&follower.card) {
                if follower.card == MONTEI {
                    self.pending.push((follower.id.clone(), "P2".to_owned()));
                }
                self.zone("P2", "cemetery").push(follower);
            } else {
                self.zone("P2", "field").push(follower);
            }
        }
        Ok(self.after_resolution())
    }

    fn legal(&self) -> Vec<Value> {
        match self.point.as_ref().map(|(_, at)| at.as_str()) {
            Some("check-timing") => self
                .pending
                .iter()
                .map(|(s, _)| json!({"do": "choose-pending", "pending": {"ability": {"source": s, "line": if s.starts_with('a') { 2 } else { 1 }}}}))
                .collect(),
            Some("resolve") => self.resolving.as_ref().map_or_else(Vec::new, |(_, o)| {
                o.iter().map(|s| json!({"do": "resolve-choice", "select": s})).collect()
            }),
            Some("quick") => {
                let mut out = vec![json!({"do": "pass"})];
                let pp = self.pp.get("P2").copied().unwrap_or(0);
                let targets: Vec<String> = self
                    .zones
                    .get(&("P1".to_owned(), "field".to_owned()))
                    .into_iter()
                    .flatten()
                    .map(|c| c.id.clone())
                    .collect();
                let hand = self.zones.get(&("P2".to_owned(), "hand".to_owned()));
                for card in hand.into_iter().flatten().filter(|c| c.card == SNIPE && pp >= 1) {
                    for t in &targets {
                        out.push(json!({"do": "play", "card": card.id, "targets": {"1": [t]}}));
                    }
                }
                out
            }
            _ => vec![json!({"do": "end-phase"})],
        }
    }
}

struct RulesAi(Rules);

impl AiEngine for RulesAi {
    fn load(&mut self, fixture: &Fixture, _seed: &str) -> Result<(), EngineError> {
        let mut rules = Rules::default();
        for (player, data) in fixture.setup["players"].as_object().unwrap() {
            rules
                .pp
                .insert(player.clone(), data["pp"]["current"].as_u64().unwrap_or(0));
            for (zone, list) in data["zones"].as_object().into_iter().flatten() {
                let cards = list
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|c| c.get("id").is_some())
                    .map(|c| Card {
                        id: c["id"].as_str().unwrap().to_owned(),
                        card: c["card"].as_str().unwrap().to_owned(),
                        damage: 0,
                    });
                rules.zone(player, zone).extend(cards);
            }
        }
        rules.shuffles = fixture.random["shuffles"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        rules.point = Some(("P1".to_owned(), "main".to_owned()));
        self.0 = rules;
        Ok(())
    }

    fn decide(&mut self, decision: &Value) -> Result<Step, EngineError> {
        let outcome = self.0.apply(decision)?;
        Ok(Step {
            outcome: outcome.to_owned(),
            events: Vec::new(),
        })
    }

    fn legal(&self) -> Result<Vec<Value>, EngineError> {
        Ok(self.0.legal())
    }

    fn projection(&self, _view: View) -> Result<Value, EngineError> {
        Ok(json!({}))
    }

    fn query(&self, _view: View, path: &str) -> Result<Option<Value>, EngineError> {
        let (player, zone) = path.split_once('.').unwrap();
        let ids: Vec<&str> = self
            .0
            .zones
            .get(&(player.to_owned(), zone.to_owned()))
            .into_iter()
            .flatten()
            .map(|c| c.id.as_str())
            .collect();
        Ok(Some(json!(ids)))
    }

    fn awaiting(&self) -> Result<Option<Value>, EngineError> {
        Ok(self
            .0
            .point
            .as_ref()
            .map(|(by, at)| json!({"by": by, "at": at})))
    }

    fn think(&mut self, _: View, _: &str, _: u64, _: &str) -> Result<AiReport, EngineError> {
        Ok(AiReport {
            decision: self.0.legal()[0].clone(),
            candidates: Vec::new(),
            trace: Vec::new(),
            edges: 0,
            engine_steps: 0,
            millis: 0,
        })
    }
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/ai-positions")
}

#[test]
fn the_positions_agree_with_hand_written_rules() {
    let positions = load_positions(&dir()).unwrap();
    let mut factory = || -> Box<dyn AiEngine> { Box::new(RulesAi(Rules::default())) };
    let reports = check_ai(&mut factory, &positions, &dir().join("none.yaml"));
    for (position, id) in [
        ("ai-h", "H2"),
        ("ai-q-a", "Q1b"),
        ("ai-q-b", "Q1b"),
        ("ai-r-a", "R2"),
        ("ai-r-b", "R2"),
    ] {
        let got = &reports
            .iter()
            .find(|r| r.position == position && r.id == id)
            .unwrap()
            .outcome;
        assert_eq!(got, &AiOutcome::Pass, "{position}/{id}");
    }
}

/// The controlled shuffle after the first search is an ordering of what is left, and
/// the second search sees the deck in that order.
#[test]
fn h2_branch_shuffles_are_applied_in_order() {
    let positions = load_positions(&dir()).unwrap();
    let h = &positions.0["h"];
    let h2 = h.checks.iter().find(|c| c.id == "H2").unwrap();
    let first = &h2.branches[0];
    for leaf in &first.branches {
        let fixture = Fixture {
            question: h.id.clone(),
            scenario: h.id.clone(),
            setup: h.setup.clone(),
            card_facts: json!({}),
            random: leaf.random.clone().unwrap(),
        };
        let mut engine = RulesAi(Rules::default());
        engine.load(&fixture, "").unwrap();
        let before = h2.before.clone().unwrap();
        let path = before
            .as_array()
            .unwrap()
            .iter()
            .chain(&first.before)
            .chain(&leaf.before);
        for decision in path {
            let step = engine.decide(decision).unwrap();
            assert!(
                !step.outcome.starts_with("cannot"),
                "{}: {decision}",
                leaf.name
            );
        }
        let deck = engine.query(View::Omniscient, "P2.deck").unwrap().unwrap();
        // The second search has not shuffled yet: the deck is the first shuffle's result.
        assert_eq!(
            deck, fixture.random["shuffles"][0]["result"],
            "{}",
            leaf.name
        );
    }
}
