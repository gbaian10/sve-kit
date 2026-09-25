//! A scripted AI engine for validating the AI checks.
//!
//! `FakeAi` answers `legal` from the positions' own expectations (an oracle, as for the
//! runner), builds projections from the setup, and thinks only from what the seat sees.
//! Each `Mutation` breaks exactly one thing a check is meant to catch.

use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::{Map, Value, json};
use sve_scenario_runner::ai::{AiEngine, AiPositions, AiReport, Before, Branch, Check};
use sve_scenario_runner::{EngineError, Fixture, Step, View};

/// One broken behaviour, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mutation {
    None,
    /// H1: the root legal set lacks one distribution.
    MissingDistribution,
    /// H1: the root legal set has an attack on a standing follower.
    ExtraAttack,
    /// H2: the second search still offers `b5` after `b5` was taken.
    StaleSearch,
    /// H3 and others: `think` returns a decision that is not legal.
    IllegalDecision,
    /// Q2: `think` depends on P2's hidden hand.
    PeekHand,
    /// R1: `think` depends on P1's hidden deck order.
    PeekDeck,
    /// R3: P2's projection shows the cards P1 is looking at.
    LeakReveal,
    /// P1: the profiles lead the wrong way.
    ProfileReversed,
    /// P1: the profile is not used.
    ProfileIgnored,
    /// P2: `think` changes the position.
    ThinkMutates,
    /// Q0b: the public deck list does not reach P1.
    NoDeckList,
    /// Q0: P1 sees P2's hand.
    LeakHand,
    /// Every call is unsupported.
    Unsupported,
    /// Every decision is rejected.
    RejectDecisions,
    /// Chosen targets and selections come in reverse order (still correct).
    ReverseTargets,
    /// `think` explores one edge more than the budget.
    OverBudget,
    /// The self-reported trace has a P2 Quick play.
    TraceQuick,
    /// The trace has near misses only.
    TracePartial,
    /// P2's projection shows the card numbers (not the ids) P1 is looking at.
    LeakRevealNumbers,
}

pub(crate) const MUTATIONS: &[Mutation] = &[
    Mutation::MissingDistribution,
    Mutation::ExtraAttack,
    Mutation::StaleSearch,
    Mutation::IllegalDecision,
    Mutation::PeekHand,
    Mutation::PeekDeck,
    Mutation::LeakReveal,
    Mutation::ProfileReversed,
    Mutation::ProfileIgnored,
    Mutation::ThinkMutates,
    Mutation::NoDeckList,
    Mutation::LeakHand,
];

/// Position id + decisions so far → the legal set.
pub(crate) type Table = BTreeMap<(String, String), Vec<Value>>;

fn key(position: &str, decisions: &[Value]) -> (String, String) {
    (
        position.to_owned(),
        Value::Array(decisions.to_vec()).to_string(),
    )
}

fn before(checks: &[Check], check: &Check) -> Vec<Value> {
    match check.before() {
        None => Vec::new(),
        Some(Before::Decisions(list)) => list,
        Some(Before::Ref(id)) => checks
            .iter()
            .find(|c| c.id == id)
            .map(|c| before(checks, c))
            .unwrap_or_default(),
    }
}

fn add_branches(table: &mut Table, position: &str, parent: &[Value], branches: &[Branch]) {
    for branch in branches {
        let mut decisions = parent.to_vec();
        decisions.extend(branch.before.iter().cloned());
        if let Some(legal) = &branch.legal_exact {
            table.insert(key(position, &decisions), legal.clone());
        }
        add_branches(table, position, &decisions, &branch.branches);
    }
}

pub(crate) fn table(positions: &AiPositions) -> Rc<Table> {
    let mut table = Table::new();
    for position in positions.0.values() {
        for check in &position.checks {
            let decisions = before(&position.checks, check);
            if let Some(legal) = &check.legal_exact {
                table.insert(key(&position.id, &decisions), legal.clone());
            }
            add_branches(&mut table, &position.id, &decisions, &check.branches);
        }
    }
    Rc::new(table)
}

pub(crate) struct FakeAi {
    mutation: Mutation,
    table: Rc<Table>,
    fixture: Option<Fixture>,
    decisions: Vec<Value>,
}

impl FakeAi {
    pub(crate) const fn new(mutation: Mutation, table: Rc<Table>) -> Self {
        Self {
            mutation,
            table,
            fixture: None,
            decisions: Vec::new(),
        }
    }

    fn fixture(&self) -> Result<&Fixture, EngineError> {
        self.fixture
            .as_ref()
            .ok_or_else(|| EngineError::Adapter("not loaded".to_owned()))
    }

    fn id(&self) -> &str {
        self.fixture.as_ref().map_or("", |f| f.question.as_str())
    }

    fn player<'fx>(fixture: &'fx Fixture, seat: &str) -> &'fx Value {
        &fixture.setup["players"][seat]
    }

    /// Cards P1 is looking at during AI-R's search: the top eight of the deck.
    fn revealed(&self, fixture: &Fixture) -> Vec<Value> {
        if !self.id().starts_with("ai-r-") || self.decisions.len() < 2 {
            return Vec::new();
        }
        let deck = Self::player(fixture, "P1")["zones"]["deck"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        deck.into_iter().take(8).collect()
    }
}

fn hidden(list: &Value) -> Value {
    let n: u64 = list.as_array().map_or(0, |items| {
        items
            .iter()
            .map(|c| c.get("filler").and_then(Value::as_u64).unwrap_or(1))
            .sum()
    });
    json!([{"filler": n}])
}

fn reverse_choices(decision: &mut Value) {
    if let Some(Value::Object(targets)) = decision.get_mut("targets") {
        for list in targets.values_mut() {
            if let Value::Array(items) = list {
                items.reverse();
            }
        }
    }
    if let Some(Value::Array(items)) = decision.get_mut("select") {
        items.reverse();
    }
}

/// Records the `random` of every load, to check what the checks pass on.
pub(crate) struct Recording {
    pub(crate) inner: FakeAi,
    pub(crate) randoms: Rc<core::cell::RefCell<Vec<Value>>>,
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "each mutation only touches the calls it targets"
)]
impl AiEngine for FakeAi {
    fn load(&mut self, fixture: &Fixture, _seed: &str) -> Result<(), EngineError> {
        if self.mutation == Mutation::Unsupported {
            return Err(EngineError::Unsupported("fake".to_owned()));
        }
        self.fixture = Some(fixture.clone());
        self.decisions.clear();
        Ok(())
    }

    fn decide(&mut self, decision: &Value) -> Result<Step, EngineError> {
        self.decisions.push(decision.clone());
        let outcome = if self.mutation == Mutation::RejectDecisions {
            "cannot-play"
        } else {
            "resolved"
        };
        Ok(Step {
            outcome: outcome.to_owned(),
            events: Vec::new(),
        })
    }

    fn legal(&self) -> Result<Vec<Value>, EngineError> {
        let mut legal = self
            .table
            .get(&key(self.id(), &self.decisions))
            .cloned()
            .unwrap_or_else(|| vec![json!({"do": "end-phase"})]);
        let root_h = self.id() == "ai-h" && self.decisions.is_empty();
        match self.mutation {
            Mutation::MissingDistribution if root_h => {
                legal.pop();
            }
            Mutation::ExtraAttack if root_h => {
                legal.push(json!({"do": "attack", "attacker": "a1", "target": "b1"}));
            }
            Mutation::StaleSearch
                if self.id() == "ai-h"
                    && self
                        .decisions
                        .iter()
                        .any(|d| d.get("select") == Some(&json!(["b5"]))) =>
            {
                legal.push(json!({"do": "resolve-choice", "select": ["b5"]}));
            }
            Mutation::ReverseTargets => legal.iter_mut().for_each(reverse_choices),
            _ => {}
        }
        Ok(legal)
    }

    fn projection(&self, view: View) -> Result<Value, EngineError> {
        let fixture = self.fixture()?;
        let open = fixture.setup["room"]["open_decklists"] == json!(true);
        let mut players = Map::new();
        for seat in ["P1", "P2"] {
            let source = Self::player(fixture, seat);
            let own = view == View::Omniscient || View::parse(seat) == Some(view);
            let mut zones = Map::new();
            for (zone, list) in source["zones"].as_object().into_iter().flatten() {
                let shown = match zone.as_str() {
                    "hand" if own || self.mutation == Mutation::LeakHand => list.clone(),
                    "deck" if view == View::Omniscient => list.clone(),
                    "hand" | "deck" => hidden(list),
                    _ => list.clone(),
                };
                zones.insert(zone.clone(), shown);
            }
            let mut player = Map::new();
            player.insert("leader".to_owned(), source["leader"].clone());
            player.insert("zones".to_owned(), Value::Object(zones));
            let list_hidden = self.mutation == Mutation::NoDeckList && view == View::P1;
            if open
                && !list_hidden
                && let Some(list) = source.get("deck_list")
            {
                player.insert("deck_list".to_owned(), list.clone());
            }
            players.insert(seat.to_owned(), Value::Object(player));
        }
        let mut out = json!({"players": players, "decisions": self.decisions});
        let sees_reveal =
            matches!(view, View::Omniscient | View::P1) || self.mutation == Mutation::LeakReveal;
        let looking = match (sees_reveal, self.mutation) {
            (true, _) => Some(self.revealed(fixture)),
            (false, Mutation::LeakRevealNumbers) => Some(
                self.revealed(fixture)
                    .iter()
                    .map(|c| c["card"].clone())
                    .collect(),
            ),
            (false, _) => None,
        };
        if let Some(cards) = looking {
            out["looking"] = Value::Array(cards);
        }
        Ok(out)
    }

    fn query(&self, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        let projection = self.projection(view)?;
        let mut node = &projection["players"];
        for part in path.split('.') {
            node = &node[part];
        }
        Ok((!node.is_null()).then(|| node.clone()))
    }

    fn think(
        &mut self,
        seat: View,
        profile: &str,
        budget: u64,
        _seed: &str,
    ) -> Result<AiReport, EngineError> {
        let legal = self.legal()?;
        let seen = self.projection(seat)?;
        let leader = |d: &&Value| d.get("target") == Some(&json!("P2.leader"));
        let follower = |d: &&Value| {
            d.get("do") == Some(&json!("attack")) && d.get("target") != Some(&json!("P2.leader"))
        };
        let wanted = match (self.mutation, profile) {
            (Mutation::ProfileReversed, "aggro") => legal.iter().find(follower),
            (Mutation::ProfileIgnored, _)
            | (Mutation::ProfileReversed, "control")
            | (_, "aggro") => legal.iter().find(leader),
            (_, "control") => legal.iter().find(follower),
            _ => None,
        };
        let mut decision = wanted
            .or_else(|| legal.first())
            .cloned()
            .unwrap_or(Value::Null);
        let mut candidates: Vec<(Value, f64)> = legal
            .iter()
            .zip((1..=legal.len()).rev())
            .map(|(d, rank)| (d.clone(), f64::from(u32::try_from(rank).unwrap_or(0))))
            .collect();
        let fixture = self.fixture()?.clone();
        match self.mutation {
            Mutation::IllegalDecision => {
                decision = json!({"do": "attack", "attacker": "zz", "target": "P2.leader"});
            }
            Mutation::PeekHand => {
                let hand = &Self::player(&fixture, "P2")["zones"]["hand"];
                if hand.to_string().contains("BP01-179")
                    && let Some(first) = candidates.first_mut()
                {
                    first.1 = -1.0;
                }
            }
            Mutation::PeekDeck if self.decisions.is_empty() => {
                let top = &Self::player(&fixture, "P1")["zones"]["deck"][4];
                if top.to_string().contains("BP01-158")
                    && let Some(first) = candidates.first_mut()
                {
                    first.1 = -2.0;
                }
            }
            Mutation::ThinkMutates => self.decisions.push(json!({"do": "noop"})),
            _ => {}
        }
        let play = json!([{"do": "play"}]);
        let trace = match self.mutation {
            Mutation::TraceQuick => vec![json!({"by": "P2", "at": "quick", "options": play})],
            Mutation::TracePartial => vec![
                json!({"by": "P2", "at": "main", "options": play}),
                json!({"by": "P1", "at": "quick", "options": play}),
                json!({"by": "P2", "at": "quick", "options": [{"do": "pass"}]}),
            ],
            _ => vec![json!({"depth": 0, "by": "P1", "at": "main", "seen": seen.is_object()})],
        };
        let edges = if self.mutation == Mutation::OverBudget {
            budget.saturating_add(1)
        } else {
            budget
        };
        Ok(AiReport {
            decision,
            candidates,
            trace,
            edges,
            engine_steps: 1,
            millis: 0,
        })
    }
}

impl AiEngine for Recording {
    fn load(&mut self, fixture: &Fixture, seed: &str) -> Result<(), EngineError> {
        self.randoms.borrow_mut().push(fixture.random.clone());
        self.inner.load(fixture, seed)
    }

    fn decide(&mut self, decision: &Value) -> Result<Step, EngineError> {
        self.inner.decide(decision)
    }

    fn legal(&self) -> Result<Vec<Value>, EngineError> {
        self.inner.legal()
    }

    fn projection(&self, view: View) -> Result<Value, EngineError> {
        self.inner.projection(view)
    }

    fn query(&self, view: View, path: &str) -> Result<Option<Value>, EngineError> {
        self.inner.query(view, path)
    }

    fn think(
        &mut self,
        seat: View,
        profile: &str,
        budget: u64,
        seed: &str,
    ) -> Result<AiReport, EngineError> {
        self.inner.think(seat, profile, budget, seed)
    }
}
