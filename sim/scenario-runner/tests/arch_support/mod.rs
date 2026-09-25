//! Scripted engines that validate the architecture checks themselves.
//!
//! They do not resolve rules: path P and the undo line of the fixture are written out
//! as tables. The replay engine keeps a real node tree (branches, saves, undo with
//! carried identities) so the checks can observe correct and broken skeletons.
//! Each mutation breaks exactly one thing a real skeleton could get wrong.

#![allow(dead_code, reason = "each test binary uses a different subset")]
#![allow(
    clippy::multiple_inherent_impl,
    clippy::struct_excessive_bools,
    reason = "a scripted test engine: tables grouped by topic, flags mirror the fixture"
)]

use core::hash::{Hash, Hasher as _};
use core::sync::atomic::{AtomicUsize, Ordering};
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;

use serde_json::{Map, Value, json};
use sve_scenario_runner::arch::{
    AssistEngine, BranchKind, Layer, Layered, NodeId, Observation, Realign, ReplayEngine,
};
use sve_scenario_runner::{EngineError, Fixture, Step, View};

static INSTANCES: AtomicUsize = AtomicUsize::new(0);

/// A broken replay skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplayMutation {
    None,
    /// The save keeps the board but not the seed or scripted shuffles.
    SaveVisibleOnly,
    /// A branch shares mutable state with its source.
    SharedBranch,
    ConstantDigest,
    /// The digest leaves out carried knowledge.
    DigestNoKnowledge,
    /// Undo forgets what was seen before it.
    UndoNoCarry,
    /// Undo keeps the undoing player's knowledge but forgets the opponent's.
    UndoForgetOpponent,
    ExportSeed,
    /// P1's export shows the deck order after the shuffle.
    ExportTopCard,
    /// P2's export shows P1's deck.
    ExportOpponentHidden,
    /// The placed card's cause points to an unknown event.
    CauseBroken,
    /// Undo discards the original line.
    UndoDeletesOriginal,
    /// Exports carry a per-instance transport counter: harmless, but only once approved.
    TransportCounter,
    /// P2's life is off by one after the attack.
    WrongLife,
    /// A replay branch hands P2 an identity it never saw.
    BranchOverShares,
    /// Branching changes what the source line shows, but not its digest.
    BranchTouchesSource,
    /// Undo is logged as a plain branch.
    UndoLoggedAsBranch,
    /// Undo rewrites the decisions of the original line.
    UndoRewritesOriginal,
    /// After an undo the original line refuses to continue.
    UndoFreezesOriginal,
    /// P1's export shows the deck order before any search.
    ExportInitialOrder,
    /// P2's awaiting during P1's search carries an extra field.
    AwaitingExtraField,
    /// An event names a decision node the engine never issued.
    CauseUnknownNode,
}

pub(crate) const REPLAY_MUTATIONS: &[(ReplayMutation, &[&str])] = &[
    (ReplayMutation::SaveVisibleOnly, &["R1"]),
    (ReplayMutation::SharedBranch, &["R3"]),
    (ReplayMutation::ConstantDigest, &["R0"]),
    (ReplayMutation::DigestNoKnowledge, &["R2b", "R4"]),
    (ReplayMutation::UndoNoCarry, &["R4"]),
    (ReplayMutation::UndoForgetOpponent, &["R4"]),
    (ReplayMutation::ExportSeed, &["R5"]),
    (ReplayMutation::ExportTopCard, &["R5"]),
    (ReplayMutation::ExportOpponentHidden, &["R5"]),
    (ReplayMutation::CauseBroken, &["R6"]),
    (ReplayMutation::UndoDeletesOriginal, &["R4"]),
    (ReplayMutation::WrongLife, &["R1", "R2b"]),
    (ReplayMutation::BranchOverShares, &["R2b"]),
    (ReplayMutation::BranchTouchesSource, &["R3"]),
    (ReplayMutation::UndoLoggedAsBranch, &["R4"]),
    (ReplayMutation::UndoRewritesOriginal, &["R4"]),
    (ReplayMutation::UndoFreezesOriginal, &["R4"]),
    (ReplayMutation::ExportInitialOrder, &["R5"]),
    (ReplayMutation::AwaitingExtraField, &["R5"]),
    (ReplayMutation::CauseUnknownNode, &["R6"]),
];

#[derive(Debug, Clone)]
struct Node {
    id: String,
    parent: Option<usize>,
    branch: usize,
    decisions: Vec<Value>,
    step: Step,
    carried: BTreeMap<String, Vec<(String, String)>>,
    dead: bool,
    touched: bool,
}

#[derive(Debug, Clone)]
struct Game {
    lookout: bool,
    deck: Vec<String>,
    cards: BTreeMap<String, String>,
    shuffle: Option<Vec<String>>,
    seed: String,
    nodes: Vec<Node>,
    /// Branch number → the node it was taken from.
    branches: Vec<Option<usize>>,
    admin: Vec<Value>,
}

/// The scripted replay engine.
pub(crate) struct FakeReplay {
    mutation: ReplayMutation,
    tag: usize,
    counter: usize,
    game: Option<Game>,
}

fn setup_list(fixture: &Fixture, player: &str, zone: &str) -> Vec<(String, String)> {
    fixture.setup["players"][player]["zones"][zone]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    Some((e["id"].as_str()?.to_owned(), e["card"].as_str()?.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn digest_of(text: &str) -> String {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn adapter(msg: &str) -> EngineError {
    EngineError::Adapter(msg.to_owned())
}

const fn view_key(view: View) -> &'static str {
    match view {
        View::Omniscient => "omniscient",
        View::P1 => "P1",
        View::P2 => "P2",
    }
}

impl FakeReplay {
    pub(crate) fn new(mutation: ReplayMutation) -> Self {
        Self {
            mutation,
            tag: INSTANCES.fetch_add(1, Ordering::Relaxed),
            counter: 0,
            game: None,
        }
    }

    fn fresh(&mut self, kind: &str) -> String {
        self.counter += 1;
        format!("i{}-{kind}{}", self.tag, self.counter)
    }

    fn game(&self) -> Result<&Game, EngineError> {
        self.game.as_ref().ok_or_else(|| adapter("no game"))
    }

    fn find(&self, id: &NodeId) -> Result<(usize, &Node), EngineError> {
        self.game()?
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.id == id.0)
            .ok_or_else(|| adapter("unknown node"))
    }

    fn ancestors(game: &Game, mut at: usize) -> Vec<usize> {
        let mut out = vec![at];
        while let Some(p) = game.nodes[at].parent {
            out.push(p);
            at = p;
        }
        out
    }

    /// An earlier event on this line matching `pattern`.
    fn earlier(game: &Game, at: usize, pattern: &Value) -> Option<String> {
        Self::ancestors(game, at).into_iter().find_map(|i| {
            game.nodes[i]
                .step
                .events
                .iter()
                .find(|e| sve_scenario_runner::compare::subset(pattern, e))
                .and_then(|e| e["id"].as_str().map(str::to_owned))
        })
    }

    fn start_game(fixture: &Fixture, seed: &str) -> Game {
        let p1_deck = setup_list(fixture, "P1", "deck");
        let lookout = p1_deck.iter().any(|(id, _)| id == "d");
        let mut cards = BTreeMap::new();
        for player in ["P1", "P2"] {
            for zone in ["deck", "hand", "field"] {
                cards.extend(setup_list(fixture, player, zone));
            }
        }
        let shuffle = fixture.random["shuffles"][0]["result"].as_array().map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        });
        Game {
            lookout,
            deck: p1_deck.into_iter().map(|(id, _)| id).collect(),
            cards,
            shuffle,
            seed: seed.to_owned(),
            nodes: Vec::new(),
            branches: vec![None],
            admin: Vec::new(),
        }
    }

    fn d5(game: &Game) -> Vec<String> {
        if let Some(order) = &game.shuffle {
            return order.clone();
        }
        let rest: Vec<String> = game.deck.iter().filter(|id| *id != "a3").cloned().collect();
        let odd = game.seed.bytes().map(usize::from).sum::<usize>() % 2 == 1;
        if odd {
            rest.into_iter().rev().collect()
        } else {
            rest
        }
    }

    fn carried_objects(node: &Node, view: &str) -> Vec<String> {
        node.carried
            .get(view)
            .map(|c| c.iter().map(|(o, _)| o.clone()).collect())
            .unwrap_or_default()
    }

    fn digest_text(&self, game: &Game, node: &Node) -> String {
        if self.mutation == ReplayMutation::ConstantDigest {
            return "constant".to_owned();
        }
        let hidden: Vec<String> = game
            .deck
            .iter()
            .map(|id| format!("{id}:{}", game.cards.get(id).cloned().unwrap_or_default()))
            .collect();
        let knowledge = if self.mutation == ReplayMutation::DigestNoKnowledge {
            String::new()
        } else {
            format!(
                "{:?}{:?}",
                Self::carried_objects(node, "P1"),
                Self::carried_objects(node, "P2")
            )
        };
        format!(
            "{hidden:?}|{}|{:?}|{}|{knowledge}",
            game.seed,
            game.shuffle,
            Value::from(node.decisions.clone())
        )
    }
}

/// Where path P (or the undo line) stands: the number of decisions and whether a3 went in acted.
struct At {
    k: usize,
    alt: bool,
    lookout: bool,
}

fn at_of(game: &Game, node: &Node) -> At {
    At {
        k: node.decisions.len(),
        alt: node
            .decisions
            .iter()
            .any(|d| d["object"] == "a3" && d["acted"] == true),
        lookout: game.lookout,
    }
}

fn identifiable(at: &At, view: &str) -> Vec<String> {
    let k = at.k;
    let mut out: Vec<&str> = Vec::new();
    if at.lookout {
        match view {
            "P1" => {
                out.push("a");
                if k >= 2 {
                    out.extend(["d", "x"]);
                }
            }
            _ => {
                if k >= 1 {
                    out.push("a");
                }
            }
        }
    } else if view == "P1" {
        out.extend(["a1", "a2", "a6"]);
        if k >= 3 {
            out.extend(["a3", "a4", "a5"]);
        }
    } else {
        out.extend(["a2", "a6"]);
        if k >= 1 {
            out.push("a1");
        }
        if k >= 4 {
            out.push("a3");
        }
        if k >= 11 {
            out.push("b1");
        }
    }
    out.into_iter().map(str::to_owned).collect()
}

fn with_cards(ids: &[String], game: &Game) -> Value {
    Value::from(
        ids.iter()
            .map(|id| json!({"id": id, "card": game.cards.get(id)}))
            .collect::<Vec<_>>(),
    )
}

fn p1_field(at: &At) -> Vec<String> {
    let k = at.k;
    let ids: &[&str] = if at.lookout {
        if k >= 1 { &["a"] } else { &[] }
    } else if k >= 5 {
        &["a2", "a6", "a1", "a3"]
    } else if k >= 2 {
        &["a2", "a6", "a1"]
    } else {
        &["a2", "a6"]
    };
    ids.iter().map(|s| (*s).to_owned()).collect()
}

fn p1_hand(at: &At, d5: &[String]) -> Vec<String> {
    let k = at.k;
    if at.lookout {
        return match k {
            0 => vec!["a".to_owned()],
            1 | 2 => Vec::new(),
            _ => vec!["d".to_owned()],
        };
    }
    match k {
        0 => vec!["a1".to_owned()],
        13.. => d5.first().cloned().into_iter().collect(),
        _ => Vec::new(),
    }
}

fn p1_deck(at: &At, game: &Game) -> Vec<String> {
    let k = at.k;
    if at.lookout {
        return if k >= 3 {
            vec!["x".to_owned()]
        } else {
            game.deck.clone()
        };
    }
    let d5 = FakeReplay::d5(game);
    match k {
        0..=4 => game.deck.clone(),
        5..=12 => d5,
        _ => d5.into_iter().skip(1).collect(),
    }
}

fn awaiting_full(at: &At) -> Value {
    let k = at.k;
    if at.lookout {
        return match k {
            0 => json!({"by": "P1", "choices": [{"do": "play", "card": "a"}, {"do": "end-phase"}]}),
            1 => {
                json!({"by": "P1", "choices": [{"do": "choose-pending", "pending": {"ability": {"source": "a", "line": 2}}}]})
            }
            2 => {
                json!({"by": "P1", "choices": [{"do": "resolve-choice", "select": ["d"]}, {"do": "resolve-choice", "select": ["x"]}]})
            }
            _ => json!({"by": "P1", "choices": [{"do": "end-phase"}]}),
        };
    }
    match k {
        1 => {
            json!({"by": "P1", "choices": [{"do": "place-acted", "object": "a1", "acted": false}, {"do": "place-acted", "object": "a1", "acted": true}]})
        }
        2 => {
            json!({"by": "P1", "choices": [{"do": "choose-pending", "pending": {"ability": {"source": "a1", "line": 2}}}]})
        }
        3 => {
            json!({"by": "P1", "choices": [{"do": "resolve-choice", "select": ["a3"]}, {"do": "resolve-choice", "select": []}]})
        }
        4 => {
            json!({"by": "P1", "choices": [{"do": "place-acted", "object": "a3", "acted": false}, {"do": "place-acted", "object": "a3", "acted": true}]})
        }
        5 => {
            json!({"by": "P1", "choices": [{"do": "choose-pending", "pending": {"ability": {"source": "a3", "line": 2}}}]})
        }
        7 | 10 => json!({"by": "P2", "choices": [{"do": "pass"}]}),
        9 => {
            json!({"by": "P1", "choices": [{"do": "guard-act", "select": []}, {"do": "guard-act", "select": ["a1"]}]})
        }
        11 => json!({"by": "P2", "choices": [{"do": "end-phase"}]}),
        12 => json!({"by": "P1", "choices": [{"do": "pass"}]}),
        _ => json!({"by": "P1", "choices": [{"do": "end-phase"}]}),
    }
}

fn awaiting_for(at: &At, view: View, extra: bool) -> Value {
    let full = awaiting_full(at);
    let by = full["by"].clone();
    match view {
        View::Omniscient => full,
        View::P1 | View::P2 if by == view_key(view) => full,
        View::P1 | View::P2 if extra && !at.lookout && at.k == 3 => json!({"by": by, "note": 1}),
        View::P1 | View::P2 => json!({"by": by}),
    }
}

const fn outcome_of(at: &At) -> &'static str {
    if at.lookout {
        return match at.k {
            2 => "paused",
            _ => "resolved",
        };
    }
    match at.k {
        1 | 3 | 4 => "paused",
        6 => "pending-cancelled",
        _ => "resolved",
    }
}

impl FakeReplay {
    const fn p2_life(&self, at: &At) -> i64 {
        match (at.lookout, at.k >= 8, self.mutation) {
            (false, true, ReplayMutation::WrongLife) => 16,
            (false, true, _) => 17,
            (true, _, _) | (false, false, _) => 20,
        }
    }

    fn projection(&self, game: &Game, node: &Node, view: View) -> Value {
        let at = at_of(game, node);
        let d5 = Self::d5(game);
        let field = p1_field(&at);
        let hand = p1_hand(&at, &d5);
        let deck = p1_deck(&at, game);
        let life = self.p2_life(&at);
        let p2_hand: Vec<String> = if !at.lookout && at.k >= 11 {
            vec!["b1".to_owned()]
        } else {
            Vec::new()
        };
        match view {
            View::Omniscient => json!({
                "P1": {"field": field, "hand": hand, "deck": deck},
                "P2": {"hand": p2_hand, "life": life},
            }),
            View::P1 => {
                let mut m = Map::new();
                m.insert("field".into(), with_cards(&field, game));
                m.insert("hand".into(), with_cards(&hand, game));
                m.insert("deck_count".into(), Value::from(deck.len()));
                m.insert("opponent_life".into(), Value::from(life));
                let searching = if at.lookout { at.k == 2 } else { at.k == 3 };
                if searching {
                    m.insert("looking_at".into(), with_cards(&deck, game));
                }
                if node.touched {
                    m.insert("touched".into(), Value::from(true));
                }
                if self.mutation == ReplayMutation::ExportInitialOrder && !at.lookout && at.k < 3 {
                    m.insert("deck_order".into(), Value::from(deck.clone()));
                }
                if self.mutation == ReplayMutation::ExportTopCard && !at.lookout && at.k >= 5 {
                    m.insert("deck_order".into(), Value::from(deck));
                }
                Value::Object(m)
            }
            View::P2 => {
                let mut m = Map::new();
                m.insert("opponent_field".into(), with_cards(&field, game));
                m.insert("opponent_hand_count".into(), Value::from(hand.len()));
                m.insert("opponent_deck_count".into(), Value::from(deck.len()));
                m.insert("hand".into(), with_cards(&p2_hand, game));
                m.insert("life".into(), Value::from(life));
                if !at.lookout && at.k >= 4 {
                    m.insert("revealed".into(), with_cards(&["a3".to_owned()], game));
                }
                if self.mutation == ReplayMutation::ExportOpponentHidden {
                    let cards: Vec<Value> = deck
                        .iter()
                        .map(|id| Value::from(game.cards.get(id).cloned()))
                        .collect();
                    m.insert("opponent_deck".into(), Value::from(cards));
                }
                Value::Object(m)
            }
        }
    }

    fn knowledge(game: &Game, node: &Node, view: View) -> Value {
        let at = at_of(game, node);
        let key = view_key(view);
        let carried: Vec<Value> = node
            .carried
            .get(key)
            .map(|c| {
                c.iter()
                    .map(|(o, from)| json!({"object": o, "from": from}))
                    .collect()
            })
            .unwrap_or_default();
        json!({"identifiable": identifiable(&at, key), "carried": carried})
    }

    /// Events of the step that produced node `index`, with causes into earlier events.
    fn events_for(&mut self, game: &Game, index: usize, at: &At, node_id: &str) -> Vec<Value> {
        let decision = json!({"decision": node_id});
        let play = |g: &Game| {
            Self::earlier(
                g,
                index,
                &json!({"kind": "プレイ", "ability": {"source": "a1", "line": 2}}),
            )
        };
        let mut events: Vec<Value> = if at.lookout {
            match at.k {
                1 => vec![json!({"kind": "プレイ", "object": "a", "cause": decision})],
                2 => vec![
                    json!({"kind": "プレイ", "ability": {"source": "a", "line": 2}, "cause": decision}),
                ],
                _ => {
                    vec![json!({"kind": "移動", "object": "d", "to": "P1.hand", "cause": decision})]
                }
            }
        } else {
            match at.k {
                1 => vec![json!({"kind": "プレイ", "object": "a1", "cause": decision})],
                2 => vec![json!({"kind": "場に出す", "object": "a1", "cause": decision})],
                3 => vec![
                    json!({"kind": "プレイ", "ability": {"source": "a1", "line": 2}, "cause": decision}),
                ],
                4 => vec![
                    json!({"kind": "公開", "object": "a3", "to": "P2", "source": "a1", "cause": {"event": play(game)}}),
                ],
                5 => vec![
                    json!({"kind": "場に出す", "object": "a3", "from": "P1.deck", "source": {"source": "a1", "line": 2}, "cause": {"event": play(game)}}),
                ],
                6 => vec![
                    json!({"kind": "待機取消", "ability": {"source": "a3", "line": 2}, "cause": decision}),
                ],
                7 if self.mutation == ReplayMutation::CauseUnknownNode => vec![
                    json!({"kind": "攻撃", "attacker": "a2", "target": "P2.leader", "cause": {"decision": "ghost-node"}}),
                ],
                7 => vec![
                    json!({"kind": "攻撃", "attacker": "a2", "target": "P2.leader", "cause": decision}),
                ],
                8 => vec![
                    json!({"kind": "ダメージ", "source": "a2", "target": "P2.leader", "amount": 3, "cause": decision}),
                ],
                11 => vec![json!({"kind": "引く", "player": "P2", "cause": {"rule": "7.2.4"}})],
                13 => vec![json!({"kind": "引く", "player": "P1", "cause": {"rule": "7.2.4"}})],
                _ => vec![json!({"kind": "フェイズ", "cause": decision})],
            }
        };
        for e in &mut events {
            e["id"] = Value::from(self.fresh("e"));
        }
        if !at.lookout {
            let first = events
                .first()
                .and_then(|e| e["id"].as_str())
                .map(str::to_owned);
            match at.k {
                2 => events.push(json!({"id": self.fresh("e"), "kind": "待機", "ability": {"source": "a1", "line": 2}, "cause": {"event": first}})),
                3 => events.push(json!({"id": self.fresh("e"), "kind": "費用成立", "ability": {"source": "a1", "line": 2}, "cause": {"event": first}})),
                5 => {
                    if self.mutation == ReplayMutation::CauseBroken {
                        for e in &mut events {
                            e["cause"] = json!({"event": "missing"});
                        }
                    }
                    events.push(json!({"id": self.fresh("e"), "kind": "シャッフル", "zone": "P1.deck", "cause": {"event": play(game)}}));
                    events.push(json!({"id": self.fresh("e"), "kind": "待機", "ability": {"source": "a3", "line": 2}, "cause": {"event": first}}));
                }
                _ => {}
            }
        }
        events
    }

    fn push_node(
        &mut self,
        parent: Option<usize>,
        branch: usize,
        decisions: Vec<Value>,
        carried: BTreeMap<String, Vec<(String, String)>>,
    ) -> Result<String, EngineError> {
        let id = self.fresh("n");
        let mut game = self.game.take().ok_or_else(|| adapter("no game"))?;
        let mut node = Node {
            id: id.clone(),
            parent,
            branch,
            decisions,
            step: Step {
                outcome: String::new(),
                events: Vec::new(),
            },
            carried,
            dead: false,
            touched: false,
        };
        let at = at_of(&game, &node);
        game.nodes.push(node.clone());
        let index = game.nodes.len() - 1;
        let events = if at.k == 0 {
            Vec::new()
        } else {
            self.events_for(&game, index, &at, &id)
        };
        node.step = Step {
            outcome: outcome_of(&at).to_owned(),
            events,
        };
        game.nodes[index] = node;
        self.game = Some(game);
        Ok(id)
    }

    fn seen(game: &Game, tail: usize, view: &str) -> Vec<String> {
        let mut out = Vec::new();
        for i in Self::ancestors(game, tail) {
            let at = at_of(game, &game.nodes[i]);
            for id in identifiable(&at, view) {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            for id in Self::carried_objects(&game.nodes[i], view) {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    }

    fn line_tail(game: &Game, branch: usize) -> Option<usize> {
        game.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.branch == branch && !n.dead)
            .map(|(i, _)| i)
            .next_back()
    }
}

impl ReplayEngine for FakeReplay {
    fn start(&mut self, fixture: &Fixture, seed: &str, _game: &str) -> Result<NodeId, EngineError> {
        self.game = Some(Self::start_game(fixture, seed));
        Ok(NodeId(self.push_node(
            None,
            0,
            Vec::new(),
            BTreeMap::new(),
        )?))
    }

    fn decide(&mut self, at: &NodeId, decision: &Value) -> Result<(NodeId, Step), EngineError> {
        let (index, node) = self.find(at)?;
        let node = node.clone();
        if node.dead {
            return Err(EngineError::Unsupported("that line was deleted".into()));
        }
        let game = self.game()?;
        if game
            .nodes
            .iter()
            .any(|n| n.parent == Some(index) && n.branch == node.branch)
        {
            return Err(adapter("not the tail of its branch"));
        }
        let undone = game.admin.iter().any(|e| e["kind"] == "undo");
        if self.mutation == ReplayMutation::UndoFreezesOriginal && undone && node.branch == 0 {
            let refused = Step {
                outcome: "cannot-play".to_owned(),
                events: Vec::new(),
            };
            return Ok((at.clone(), refused));
        }
        let mut decisions = node.decisions.clone();
        decisions.push(decision.clone());
        let source = game.branches.get(node.branch).copied().flatten();
        let id = self.push_node(Some(index), node.branch, decisions.clone(), node.carried)?;
        if self.mutation == ReplayMutation::SharedBranch
            && let Some(src) = source
            && let Some(g) = self.game.as_mut()
        {
            let src_branch = g.nodes[src].branch;
            let depth = decisions.len();
            if let Some(shared) = g
                .nodes
                .iter_mut()
                .find(|n| n.branch == src_branch && n.decisions.len() == depth)
            {
                shared.decisions = decisions;
            }
        }
        let (_, created) = self.find(&NodeId(id.clone()))?;
        Ok((NodeId(id), created.step.clone()))
    }

    fn branch(&mut self, at: &NodeId, kind: BranchKind) -> Result<NodeId, EngineError> {
        let (index, node) = self.find(at)?;
        let node = node.clone();
        let game = self.game()?;
        let tail = Self::line_tail(game, node.branch).unwrap_or(index);
        let here = at_of(game, &node);
        let mut carried: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for view in ["P1", "P2"] {
            let skip = matches!(
                (kind, view, self.mutation),
                (BranchKind::Undo, _, ReplayMutation::UndoNoCarry)
                    | (BranchKind::Undo, "P2", ReplayMutation::UndoForgetOpponent)
            );
            if skip {
                continue;
            }
            let now = identifiable(&here, view);
            let from = game.nodes[tail].id.clone();
            let brought: Vec<(String, String)> = Self::seen(game, tail, view)
                .into_iter()
                .filter(|id| !now.contains(id))
                .map(|id| (id, from.clone()))
                .collect();
            let mut brought = brought;
            if kind == BranchKind::Replay
                && view == "P2"
                && self.mutation == ReplayMutation::BranchOverShares
            {
                brought.push(("a4".to_owned(), from.clone()));
            }
            if !brought.is_empty() {
                carried.insert(view.to_owned(), brought);
            }
        }
        let new_branch = game.branches.len();
        let source_branch = node.branch;
        if let Some(g) = self.game.as_mut() {
            g.branches.push(Some(index));
            let depth = node.decisions.len();
            for n in g
                .nodes
                .iter_mut()
                .filter(|n| n.branch == source_branch && n.decisions.len() > depth)
            {
                match (kind, self.mutation) {
                    (BranchKind::Undo, ReplayMutation::UndoDeletesOriginal) => n.dead = true,
                    (BranchKind::Undo, ReplayMutation::UndoRewritesOriginal) => {
                        n.decisions.reverse();
                    }
                    (BranchKind::Replay, ReplayMutation::BranchTouchesSource) => n.touched = true,
                    _ => {}
                }
            }
        }
        let id = self.push_node(Some(index), new_branch, node.decisions, carried)?;
        if let Some(g) = self.game.as_mut() {
            let what = if kind == BranchKind::Undo
                && self.mutation != ReplayMutation::UndoLoggedAsBranch
            {
                "undo"
            } else {
                "branch"
            };
            g.admin.push(json!({"kind": what, "from": at.0, "to": id}));
        }
        Ok(NodeId(id))
    }

    fn save(&self, at: &NodeId) -> Result<Vec<u8>, EngineError> {
        let (_, node) = self.find(at)?;
        let game = self.game()?;
        let visible_only = self.mutation == ReplayMutation::SaveVisibleOnly;
        let blob = json!({
            "lookout": game.lookout,
            "deck": game.deck,
            "cards": game.cards,
            "shuffle": if visible_only { Value::Null } else { Value::from(game.shuffle.clone()) },
            "seed": if visible_only { "" } else { game.seed.as_str() },
            "decisions": node.decisions,
        });
        serde_json::to_vec(&blob).map_err(|e| adapter(&e.to_string()))
    }

    fn restore(&mut self, blob: &[u8]) -> Result<NodeId, EngineError> {
        let v: Value = serde_json::from_slice(blob).map_err(|e| adapter(&e.to_string()))?;
        let strings = |x: &Value| -> Vec<String> {
            x.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let cards = v["cards"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, c)| (k.clone(), c.as_str().unwrap_or_default().to_owned()))
                    .collect()
            })
            .unwrap_or_default();
        self.game = Some(Game {
            lookout: v["lookout"] == true,
            deck: strings(&v["deck"]),
            cards,
            shuffle: v["shuffle"].as_array().map(|_| strings(&v["shuffle"])),
            seed: v["seed"].as_str().unwrap_or_default().to_owned(),
            nodes: Vec::new(),
            branches: vec![None],
            admin: Vec::new(),
        });
        let mut id = self.push_node(None, 0, Vec::new(), BTreeMap::new())?;
        for decision in v["decisions"].as_array().cloned().unwrap_or_default() {
            id = self.decide(&NodeId(id), &decision)?.0.0;
        }
        Ok(NodeId(id))
    }

    fn export(&self, at: &NodeId, view: View) -> Result<Value, EngineError> {
        let (_, node) = self.find(at)?;
        let game = self.game()?;
        let mut out = json!({
            "node": node.id,
            "state": self.projection(game, node, view),
            "knowledge": Self::knowledge(game, node, view),
        });
        if self.mutation == ReplayMutation::ExportSeed {
            out["rng"] = Value::from(game.seed.clone());
        }
        if self.mutation == ReplayMutation::TransportCounter {
            out["transport"] = Value::from(format!("t{}", self.tag));
        }
        Ok(out)
    }

    fn digest(&self, at: &NodeId) -> Result<String, EngineError> {
        let (_, node) = self.find(at)?;
        Ok(digest_of(&self.digest_text(self.game()?, node)))
    }

    fn observe(&self, at: &NodeId, view: View) -> Result<Observation, EngineError> {
        let (_, node) = self.find(at)?;
        let game = self.game()?;
        let here = at_of(game, node);
        Ok(Observation {
            projection: self.projection(game, node, view),
            awaiting: Some(awaiting_for(
                &here,
                view,
                self.mutation == ReplayMutation::AwaitingExtraField,
            )),
            knowledge: Self::knowledge(game, node, view),
        })
    }

    fn query(&self, at: &NodeId, _view: View, path: &str) -> Result<Option<Value>, EngineError> {
        let (_, node) = self.find(at)?;
        let game = self.game()?;
        let here = at_of(game, node);
        let d5 = Self::d5(game);
        let p2_life = self.p2_life(&here);
        let deck = p1_deck(&here, game);
        let hand = p1_hand(&here, &d5);
        let p2_hand = if !here.lookout && here.k >= 11 {
            json!(["b1", {"filler": 3}])
        } else {
            json!([{"filler": 3}])
        };
        Ok(Some(match path {
            "P1.pp.current" => Value::from(match here.k {
                0 => 4,
                1 | 2 => 3,
                3..=12 => 1,
                _ => 5,
            }),
            "P1.field" => Value::from(p1_field(&here)),
            "P1.deck" => Value::from(deck),
            "P1.hand" => Value::from(hand),
            "P1.hand_count" => Value::from(hand.len()),
            "P1.deck_count" => Value::from(deck.len()),
            "P2.leader.life" => Value::from(p2_life),
            "P2.hand" => p2_hand,
            "P2.hand_count" => Value::from(0),
            "P2.deck_count" => Value::from(20),
            "P2.field" => json!([]),
            "P1.field.a3" => json!({"acted": here.alt}),
            _ => return Ok(None),
        }))
    }

    fn decisions(&self, at: &NodeId) -> Result<Vec<Value>, EngineError> {
        let (_, node) = self.find(at)?;
        Ok(if node.dead {
            Vec::new()
        } else {
            node.decisions.clone()
        })
    }

    fn events(&self, at: &NodeId) -> Result<Vec<Value>, EngineError> {
        let (_, node) = self.find(at)?;
        Ok(node.step.events.clone())
    }

    fn admin_log(&self) -> Result<Vec<Value>, EngineError> {
        Ok(self.game()?.admin.clone())
    }
}

/// A broken assist skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssistMutation {
    None,
    RewindDropsRecord,
    RewindRewritesExpected,
    AdvanceWhileDiverged,
    /// Adopt is recorded but the shadow keeps its own state.
    AdoptIgnored,
    /// Adopt also clears the pending trigger.
    AdoptClearsPending,
    /// The skipped trigger is recorded with the wrong kind and clause.
    MissedWrongKind,
    /// Rewind realigns the layers but not to the state before the divergence.
    RewindDrifts,
    MismatchWrongKind,
    MismatchWrongExpected,
    MismatchWrongActual,
    MismatchRecordedTwice,
}

pub(crate) const ASSIST_MUTATIONS: &[(AssistMutation, &[&str])] = &[
    (AssistMutation::RewindDropsRecord, &["A3", "B2"]),
    (AssistMutation::RewindRewritesExpected, &["A3", "B2"]),
    (AssistMutation::AdvanceWhileDiverged, &["A2"]),
    (AssistMutation::AdoptIgnored, &["A4", "B3"]),
    (AssistMutation::AdoptClearsPending, &["A4"]),
    (AssistMutation::MissedWrongKind, &["A1"]),
    (AssistMutation::RewindDrifts, &["A3"]),
    (AssistMutation::MismatchWrongKind, &["B1"]),
    (AssistMutation::MismatchWrongExpected, &["B1"]),
    (AssistMutation::MismatchWrongActual, &["B1"]),
    (AssistMutation::MismatchRecordedTwice, &["B1"]),
];

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
struct Layer1 {
    k: usize,
    a2_acted: bool,
    life: i64,
    quick_pending: bool,
    a6_attacked: bool,
    cleared: bool,
}

/// The scripted assist engine.
pub(crate) struct FakeAssist {
    mutation: AssistMutation,
    counter: usize,
    sandbox: Layer1,
    shadow: Layer1,
    divergences: Vec<Value>,
}

impl FakeAssist {
    pub(crate) fn new(mutation: AssistMutation) -> Self {
        Self {
            mutation,
            counter: 0,
            sandbox: Layer1::default(),
            shadow: Layer1::default(),
            divergences: Vec::new(),
        }
    }

    const fn state(&self, layer: Layer) -> &Layer1 {
        match layer {
            Layer::Sandbox => &self.sandbox,
            Layer::Shadow => &self.shadow,
        }
    }

    fn unresolved(&self) -> bool {
        self.divergences.iter().any(|d| d["resolved"].is_null())
    }

    fn apply(&mut self, which: Layer, op: &Value) -> Step {
        self.counter += 1;
        let tag = self.counter;
        let state = match which {
            Layer::Sandbox => &mut self.sandbox,
            Layer::Shadow => &mut self.shadow,
        };
        let step = |outcome: &str, events: Vec<Value>| Step {
            outcome: outcome.to_owned(),
            events: events
                .into_iter()
                .enumerate()
                .map(|(i, mut e)| {
                    e["id"] = Value::from(format!("{which:?}-{tag}-{i}"));
                    e
                })
                .collect(),
        };
        if op["attacker"] == "a6" {
            if state.k == 2 && !state.cleared {
                return step("cannot-attack", Vec::new());
            }
            state.quick_pending = true;
            state.a6_attacked = true;
            return step(
                "resolved",
                vec![json!({"kind": "攻撃", "attacker": "a6", "target": "P2.leader"})],
            );
        }
        if state.quick_pending && op["do"] == "pass" {
            state.quick_pending = false;
            state.life -= 3;
            return step(
                "resolved",
                vec![
                    json!({"kind": "ダメージ", "source": "a6", "target": "P2.leader", "amount": 3}),
                ],
            );
        }
        state.k += 1;
        let events = match state.k {
            3 => vec![
                json!({"kind": "プレイ", "ability": {"source": "a1", "line": 2}}),
                json!({"kind": "費用成立", "ability": {"source": "a1", "line": 2}}),
            ],
            4 => vec![json!({"kind": "公開", "object": "a3", "to": "P2", "source": "a1"})],
            5 => vec![
                json!({"kind": "場に出す", "object": "a3", "from": "P1.deck"}),
                json!({"kind": "待機", "ability": {"source": "a3", "line": 2}}),
            ],
            6 => vec![json!({"kind": "待機取消", "ability": {"source": "a3", "line": 2}})],
            7 => {
                state.a2_acted = true;
                vec![json!({"kind": "攻撃", "attacker": "a2", "target": "P2.leader"})]
            }
            8 => {
                state.life -= 3;
                vec![
                    json!({"kind": "ダメージ", "source": "a2", "target": "P2.leader", "amount": 3}),
                ]
            }
            _ => Vec::new(),
        };
        let at = At {
            k: state.k,
            alt: false,
            lookout: false,
        };
        step(outcome_of(&at), events)
    }

    fn record(&mut self, kind: &str, expected: &Value, actual: &Value, refs: &[&str]) {
        self.counter += 1;
        self.divergences.push(json!({
            "id": format!("dv{}", self.counter),
            "kind": kind,
            "expected": expected,
            "actual": actual,
            "refs": refs,
            "resolved": null,
        }));
    }
}

impl AssistEngine for FakeAssist {
    fn start(&mut self, _fixture: &Fixture, _seed: &str, _game: &str) -> Result<(), EngineError> {
        let fresh = Layer1 {
            life: 20,
            ..Layer1::default()
        };
        self.sandbox = fresh.clone();
        self.shadow = fresh;
        self.divergences.clear();
        Ok(())
    }

    fn act(&mut self, op: &Value) -> Result<Layered<Step>, EngineError> {
        if op["manual"] == true {
            let diverged = Step {
                outcome: "diverged".to_owned(),
                events: Vec::new(),
            };
            if op["do"] == "attack" {
                self.sandbox.a2_acted = true;
                let (kind, refs): (&str, &[&str]) =
                    if self.mutation == AssistMutation::MissedWrongKind {
                        ("mismatch", &["1.1"])
                    } else {
                        ("missed", &["7.3.4", "10.7.3.1"])
                    };
                self.record(
                    kind,
                    &json!({"pending": {"ability": {"source": "a1", "line": 2}}}),
                    op,
                    refs,
                );
            } else {
                let value = op["value"].as_i64().unwrap_or_default();
                self.sandbox.life = value;
                let m = self.mutation;
                let kind = if m == AssistMutation::MismatchWrongKind {
                    "missed"
                } else {
                    "mismatch"
                };
                let expected = if m == AssistMutation::MismatchWrongExpected {
                    0
                } else {
                    self.shadow.life
                };
                let actual = if m == AssistMutation::MismatchWrongActual {
                    0
                } else {
                    value
                };
                let times = if m == AssistMutation::MismatchRecordedTwice {
                    2
                } else {
                    1
                };
                for _ in 0..times {
                    self.record(
                        kind,
                        &json!({"path": op["path"], "value": expected}),
                        &json!({"path": op["path"], "value": actual}),
                        &["5.14"],
                    );
                }
            }
            return Ok(Layered {
                sandbox: Step {
                    outcome: "resolved".to_owned(),
                    events: Vec::new(),
                },
                shadow: diverged,
            });
        }
        let sandbox = self.apply(Layer::Sandbox, op);
        let shadow = self.apply(Layer::Shadow, op);
        Ok(Layered { sandbox, shadow })
    }

    fn divergences(&self) -> Result<Vec<Value>, EngineError> {
        Ok(self.divergences.clone())
    }

    fn auto_advance(&self) -> Result<bool, EngineError> {
        Ok(!self.unresolved())
    }

    fn advance(&mut self) -> Result<Option<Layered<Step>>, EngineError> {
        if self.unresolved() && self.mutation != AssistMutation::AdvanceWhileDiverged {
            return Ok(None);
        }
        self.shadow.k += 1;
        let step = Step {
            outcome: "resolved".to_owned(),
            events: Vec::new(),
        };
        Ok(Some(Layered {
            sandbox: step.clone(),
            shadow: step,
        }))
    }

    fn realign(&mut self, divergence: &str, mode: Realign) -> Result<(), EngineError> {
        let index = self
            .divergences
            .iter()
            .position(|d| d["id"] == divergence)
            .ok_or_else(|| adapter("unknown divergence"))?;
        match mode {
            Realign::Rewind => {
                self.sandbox = self.shadow.clone();
                if self.mutation == AssistMutation::RewindDropsRecord {
                    self.divergences.remove(index);
                    return Ok(());
                }
                if self.mutation == AssistMutation::RewindRewritesExpected {
                    self.divergences[index]["expected"] = Value::Null;
                }
                if self.mutation == AssistMutation::RewindDrifts {
                    self.sandbox.a6_attacked = true;
                    self.shadow.a6_attacked = true;
                }
                self.divergences[index]["resolved"] = Value::from("rewind");
            }
            Realign::Adopt => {
                if self.mutation == AssistMutation::AdoptClearsPending {
                    self.shadow = Layer1 {
                        cleared: true,
                        ..self.sandbox.clone()
                    };
                    self.sandbox = self.shadow.clone();
                } else if self.mutation != AssistMutation::AdoptIgnored {
                    self.shadow = self.sandbox.clone();
                } else {
                    // Recorded as adopted, but the shadow keeps its own state.
                }
                self.divergences[index]["resolved"] = Value::from("adopt");
            }
        }
        Ok(())
    }

    fn digest(&self, layer: Layer) -> Result<String, EngineError> {
        Ok(digest_of(&format!("{:?}", self.state(layer))))
    }

    fn observe(&self, layer: Layer, view: View) -> Result<Observation, EngineError> {
        let state = self.state(layer);
        let awaiting = if state.quick_pending {
            json!({"by": "P2", "choices": [{"do": "pass"}]})
        } else if state.cleared && state.k == 2 {
            json!({"by": "P1", "choices": [{"do": "end-phase"}]})
        } else {
            awaiting_full(&At {
                k: state.k,
                alt: false,
                lookout: false,
            })
        };
        let awaiting = match view {
            View::P2 if awaiting["by"] != "P2" => json!({"by": awaiting["by"]}),
            View::Omniscient | View::P1 | View::P2 => awaiting,
        };
        Ok(Observation {
            projection: json!({"k": state.k}),
            awaiting: Some(awaiting),
            knowledge: json!({"identifiable": [], "carried": []}),
        })
    }

    fn query(&self, layer: Layer, _view: View, path: &str) -> Result<Option<Value>, EngineError> {
        let state = self.state(layer);
        Ok(match path {
            "P1.field.a2" => Some(json!({"acted": state.a2_acted})),
            "P2.leader.life" => Some(Value::from(state.life)),
            "P1.field" => Some(Value::from(p1_field(&At {
                k: state.k,
                alt: false,
                lookout: false,
            }))),
            _ => None,
        })
    }
}
