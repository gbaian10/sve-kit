//! Shared fixture for the two architecture prototypes of phase D: replay and assist.
//!
//! Both designers wrap their prototype in [`ReplayEngine`] and [`AssistEngine`];
//! [`check_replay`] and [`check_assist`] run the same positions and report each
//! assertion (R0–R6, A1–A4, B1–B3) on its own. Only externally observable behaviour
//! is checked: how states, branches and saves are represented is the engine's business.
//! The data lives in `tests/architecture-fixtures/`.

mod assist;
pub mod positions;
mod replay;
mod support;

use alloc::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::ScenarioError;
use crate::engine::{EngineError, Step, View};
use crate::inherit::{Fixture, expand};
use crate::model::{from_yaml, parse_question};

pub use assist::check_assist;
pub use replay::check_replay;

/// An engine-issued node id: opaque, immutable once created, unique across branches.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(pub String);

/// How a new branch is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchKind {
    /// Undo agreed by both players, within the same game.
    Undo,
    /// A new line taken from a replay.
    Replay,
}

/// How a divergence between sandbox and shadow is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Realign {
    /// The sandbox goes back to the state before the divergence.
    Rewind,
    /// The shadow accepts the sandbox state; the divergence stays on record.
    Adopt,
}

/// Which state of an assisted game to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// What the players actually did.
    Sandbox,
    /// What the rules say should have happened.
    Shadow,
}

/// One result per layer.
#[derive(Debug, Clone)]
pub struct Layered<T> {
    /// The sandbox result.
    pub sandbox: T,
    /// The shadow result; for a manual operation, how the shadow judged it.
    pub shadow: T,
}

/// One layer's result of applying one operation.
#[derive(Debug, Clone)]
pub struct Applied {
    /// The id this layer gave the decision it applied. Events of this and later steps
    /// name it in `cause.decision`.
    pub decision: String,
    /// Outcome and events, as for the runner.
    pub step: Step,
}

/// What one viewer sees at one position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// The player payload, in any shape.
    pub projection: Value,
    /// The awaited decision as in the runner: full for the deciding player, `{by}` otherwise.
    pub awaiting: Option<Value>,
    /// `{identifiable: [ids], carried: [{object, from}]}`: `carried` are identities
    /// brought over from the source branch of an undo or replay branch.
    pub knowledge: Value,
}

/// A replay prototype, wrapped by its adapter.
///
/// `save` and `restore` may keep only the opening snapshot and the decisions and
/// replay them; they must not rely on anything the adapter stores on the side.
pub trait ReplayEngine {
    /// Opens a game; returns its root node.
    ///
    /// # Errors
    /// When the adapter cannot build the position.
    fn start(&mut self, fixture: &Fixture, seed: &str, game: &str) -> Result<NodeId, EngineError>;
    /// Appends a decision at `at`, which must be the tail of its branch.
    ///
    /// # Errors
    /// When the adapter cannot submit it; an illegal decision is a `cannot-*` outcome.
    fn decide(&mut self, at: &NodeId, decision: &Value) -> Result<(NodeId, Step), EngineError>;
    /// Starts a new branch at `at`; returns the new branch's root.
    ///
    /// # Errors
    /// When the engine cannot branch there.
    fn branch(&mut self, at: &NodeId, kind: BranchKind) -> Result<NodeId, EngineError>;
    /// Server-side save of a position, hidden information and seed included.
    ///
    /// # Errors
    /// When the engine cannot save.
    fn save(&self, at: &NodeId) -> Result<Vec<u8>, EngineError>;
    /// Loads a save into this (fresh) instance.
    ///
    /// # Errors
    /// When the blob cannot be loaded.
    fn restore(&mut self, blob: &[u8]) -> Result<NodeId, EngineError>;
    /// The data actually handed to that player's client, or to an AI in that seat.
    ///
    /// # Errors
    /// When the node is unknown.
    fn export(&self, at: &NodeId, view: View) -> Result<Value, EngineError>;
    /// Hash of the full normalized semantic state (knowledge and random state included).
    ///
    /// # Errors
    /// When the node is unknown.
    fn digest(&self, at: &NodeId) -> Result<String, EngineError>;
    /// What `view` sees at `at`.
    ///
    /// # Errors
    /// When the node is unknown.
    fn observe(&self, at: &NodeId, view: View) -> Result<Observation, EngineError>;
    /// An `assert` path as in the runner.
    ///
    /// # Errors
    /// When the node is unknown.
    fn query(&self, at: &NodeId, view: View, path: &str) -> Result<Option<Value>, EngineError>;
    /// The decisions from the start of the game to `at`, on its branch.
    ///
    /// # Errors
    /// When the node is unknown.
    fn decisions(&self, at: &NodeId) -> Result<Vec<Value>, EngineError>;
    /// Events from the previous node to `at`, each with `id` and `cause`.
    ///
    /// # Errors
    /// When the node is unknown.
    fn events(&self, at: &NodeId) -> Result<Vec<Value>, EngineError>;
    /// Record-layer operations: `{kind: undo | branch, ...}`.
    ///
    /// # Errors
    /// When the log cannot be read.
    fn admin_log(&self) -> Result<Vec<Value>, EngineError>;
}

/// An assisted game: a sandbox that accepts anything, and a shadow that follows the rules.
pub trait AssistEngine {
    /// Opens a game.
    ///
    /// # Errors
    /// When the adapter cannot build the position.
    fn start(&mut self, fixture: &Fixture, seed: &str, game: &str) -> Result<(), EngineError>;
    /// A player operation. Without `manual`, a decision checked by the rules (illegal:
    /// `cannot-*`, neither layer changes). With `manual: true`, the sandbox does it and
    /// the shadow judges it.
    ///
    /// # Errors
    /// When the adapter cannot submit it.
    fn act(&mut self, op: &Value) -> Result<Layered<Applied>, EngineError>;
    /// Every divergence ever recorded, resolved ones included.
    ///
    /// # Errors
    /// When the record cannot be read.
    fn divergences(&self) -> Result<Vec<Value>, EngineError>;
    /// Whether the engine may advance on its own (automatic handling, AI).
    ///
    /// # Errors
    /// When the state cannot be read.
    fn auto_advance(&self) -> Result<bool, EngineError>;
    /// Asks the engine to advance one step; `None` when it refuses, as it must while a
    /// divergence is unresolved.
    ///
    /// # Errors
    /// When the adapter fails.
    fn advance(&mut self) -> Result<Option<Layered<Applied>>, EngineError>;
    /// Resolves one divergence.
    ///
    /// # Errors
    /// When the divergence is unknown.
    fn realign(&mut self, divergence: &str, mode: Realign) -> Result<(), EngineError>;
    /// Semantic digest of one layer.
    ///
    /// # Errors
    /// When the state cannot be read.
    fn digest(&self, layer: Layer) -> Result<String, EngineError>;
    /// What `view` sees in one layer.
    ///
    /// # Errors
    /// When the state cannot be read.
    fn observe(&self, layer: Layer, view: View) -> Result<Observation, EngineError>;
    /// An `assert` path in one layer.
    ///
    /// # Errors
    /// When the state cannot be read.
    fn query(&self, layer: Layer, view: View, path: &str) -> Result<Option<Value>, EngineError>;
}

/// A fresh replay engine per call: some assertions need an instance that never saw the game.
pub type ReplayFactory<'factory> = dyn FnMut() -> Box<dyn ReplayEngine> + 'factory;
/// A fresh assist engine per call.
pub type AssistFactory<'factory> = dyn FnMut() -> Box<dyn AssistEngine> + 'factory;

/// Result of one assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The assertion holds.
    Pass,
    /// The assertion does not hold; one line per mismatch.
    Fail {
        /// What did not match.
        reasons: Vec<String>,
    },
    /// The prototype does not implement something the assertion needs.
    Unsupported {
        /// Engine's explanation.
        reason: String,
    },
    /// The adapter failed; not counted as a failure of the prototype.
    AdapterError {
        /// Adapter's explanation.
        reason: String,
    },
}

/// One reported assertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    /// `R0`…`R6`, `A1`…`A4`, `B1`…`B3`.
    pub id: &'static str,
    /// The result.
    pub outcome: Outcome,
}

impl CheckReport {
    fn from_result(id: &'static str, result: Result<Vec<String>, EngineError>) -> Self {
        let outcome = match result {
            Ok(reasons) if reasons.is_empty() => Outcome::Pass,
            Ok(reasons) => Outcome::Fail { reasons },
            Err(EngineError::Unsupported(reason)) => Outcome::Unsupported { reason },
            Err(EngineError::Adapter(reason)) => Outcome::AdapterError { reason },
        };
        Self { id, outcome }
    }
}

/// Paths in player payloads that the third party has approved before running.
///
/// Paths are dotted, `*` matches any key or list element. Approval is fixed before the
/// paired checks run; a path added after seeing a failure is not accepted.
#[derive(Debug, Clone)]
pub struct ArchOptions {
    /// Fields holding node ids (normalized before paired comparisons).
    pub node_id_paths: Vec<String>,
    /// Engine-specific identifiers (transport counters and the like) left out of paired
    /// comparisons; the leak scan still sees them.
    pub skip_paths: Vec<String>,
}

impl Default for ArchOptions {
    fn default() -> Self {
        Self {
            node_id_paths: vec!["knowledge.carried.*.from".to_owned()],
            skip_paths: Vec::new(),
        }
    }
}

/// Everything the checks need: positions and third-party expectations.
#[derive(Debug, Clone)]
pub struct ArchFixtures {
    /// `checks.yaml`.
    pub spec: ArchSpec,
    /// `positions.yaml`, expanded, by scenario name.
    pub positions: BTreeMap<String, Fixture>,
}

impl ArchFixtures {
    fn position(&self, name: &str) -> Result<&Fixture, EngineError> {
        self.positions
            .get(name)
            .ok_or_else(|| EngineError::Adapter(format!("fixture has no position {name:?}")))
    }
}

/// `checks.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ArchSpec {
    /// Game id given to `start`; paired positions share it.
    pub game: String,
    /// The seed for every non-scripted random choice.
    pub seed: String,
    /// A second seed (R0).
    pub alt_seed: String,
    /// Replay expectations.
    pub replay: ReplaySpec,
    /// Assist expectations.
    pub assist: AssistSpec,
}

/// Replay part of `checks.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReplaySpec {
    /// Path P, P1–P13.
    pub path: Vec<Value>,
    /// R3's alternative fifth decision.
    pub alt_step5: Value,
    /// Third-party assertions at fixed nodes.
    pub fixed: Vec<Fixed>,
    /// Node where P1 is searching (R5 pair 2).
    pub search_node: usize,
    /// The search decision kind.
    pub search_do: String,
    /// Identities each view may not know, by node range.
    pub hidden: Vec<Hidden>,
    /// Node of the replay branch in R2 and R2b.
    pub branch_from: usize,
    /// Objects each view must know on the R2b branch.
    pub branch_known: BTreeMap<String, Vec<String>>,
    /// Objects each view must not know on the R2b branch.
    pub branch_unknown: BTreeMap<String, Vec<String>>,
    /// R6.
    pub cause: CauseSpec,
    /// R4.
    pub undo: UndoSpec,
}

/// One third-party assertion at one node (omniscient view).
#[derive(Debug, Clone, Deserialize)]
pub struct Fixed {
    /// Node index on path P.
    pub node: usize,
    /// Path → value, compared as the runner does.
    #[serde(default)]
    pub assert: Map<String, Value>,
    /// The awaited decision with its complete option set.
    #[serde(default)]
    pub awaiting_exact: Option<Value>,
    /// The awaiting player and one of the options' `do`.
    #[serde(default)]
    pub awaiting_do: Option<AwaitingDo>,
    /// Path → an object id the list must contain.
    #[serde(default)]
    pub contains: Map<String, Value>,
    /// Records the deck order after the shuffle as `D5`.
    #[serde(default)]
    pub record_deck: Option<RecordDeck>,
    /// The top card of `D5` has been drawn.
    #[serde(default)]
    pub drew_recorded: Option<DrewRecorded>,
}

/// See [`Fixed::awaiting_do`].
#[derive(Debug, Clone, Deserialize)]
pub struct AwaitingDo {
    /// `P1` or `P2`.
    pub by: String,
    /// One option must have this `do`.
    #[serde(rename = "do")]
    pub what: String,
}

/// See [`Fixed::record_deck`].
#[derive(Debug, Clone, Deserialize)]
pub struct RecordDeck {
    /// Deck path.
    pub path: String,
    /// The deck must be an ordering of exactly these.
    pub of: Vec<String>,
}

/// See [`Fixed::drew_recorded`].
#[derive(Debug, Clone, Deserialize)]
pub struct DrewRecorded {
    /// Hand path.
    pub hand: String,
    /// Deck path.
    pub deck: String,
}

/// Objects `view` may not identify at nodes `from..=to`.
#[derive(Debug, Clone, Deserialize)]
pub struct Hidden {
    /// `P1` or `P2`.
    pub view: String,
    /// Object ids.
    pub objects: Vec<String>,
    /// First node.
    pub from: usize,
    /// Last node.
    pub to: usize,
}

/// R6 expectations.
#[derive(Debug, Clone, Deserialize)]
pub struct CauseSpec {
    /// The event whose chain is followed.
    pub placed: Value,
    /// The ability the chain must pass through.
    pub ability: Value,
    /// The step whose node the chain must end at.
    pub ability_step: usize,
    /// A triggered event whose chain must reach `placed`.
    pub trigger: Value,
}

/// R4 expectations.
#[derive(Debug, Clone, Deserialize)]
pub struct UndoSpec {
    /// Decisions played before the undo.
    pub path: Vec<Value>,
    /// A decision the original tail must still accept.
    pub continue_with: Value,
    /// Identities each view must carry after the undo.
    pub known: BTreeMap<String, Vec<String>>,
    /// Identities each view must not know after the undo.
    pub unknown: BTreeMap<String, Vec<String>>,
    /// Public board paths that must equal the original start.
    pub board: Vec<String>,
    /// Identities each view may not know on the original line.
    pub hidden: Vec<Hidden>,
}

/// Assist part of `checks.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct AssistSpec {
    /// Decisions of path P before the manual attack.
    pub prefix: usize,
    /// The manual attack of sequence A.
    pub manual_attack: Value,
    /// Path of the attacker.
    pub a2_path: String,
    /// The pending ability the shadow still waits for.
    pub pending: Value,
    /// One of these must be in the divergence's `refs`.
    pub refs_any: Vec<String>,
    /// A3 continues path P after Rewind up to this node. Each step is compared with the
    /// same step of path P played by the design's own replay engine.
    pub resume_to: usize,
    /// An attack legal only without a pending trigger.
    pub other_attack: Value,
    /// The opponent's pass after it.
    pub opponent_pass: Value,
    /// The search decision kind.
    pub search_do: String,
    /// Decisions of path P before the manual override.
    pub override_after: usize,
    /// The manual override of sequence B.
    pub manual_set: Value,
    /// The overridden path.
    pub life_path: String,
    /// Value by the rules.
    pub life_shadow: i64,
    /// Value set by hand.
    pub life_manual: i64,
    /// Value after Rewind and one more attack.
    pub life_after_rewind: i64,
    /// Value after Adopt and one more attack.
    pub life_after_adopt: i64,
}

/// Loads `positions.yaml` and `checks.yaml` from a directory.
///
/// # Errors
/// When a file cannot be read or does not have the expected shape.
pub fn load_fixtures(dir: &Path) -> Result<ArchFixtures, ScenarioError> {
    let read = |name: &str| {
        let path = dir.join(name);
        fs::read_to_string(&path).map_err(|e| ScenarioError::Io(format!("{}: {e}", path.display())))
    };
    let spec: ArchSpec =
        from_yaml(&read("checks.yaml")?).map_err(|e| ScenarioError::Question(e.to_string()))?;
    let question = parse_question(&read("positions.yaml")?)?;
    let positions = expand(&question.id, &question.scenarios)?
        .into_iter()
        .map(|f| (f.scenario.clone(), f))
        .collect();
    Ok(ArchFixtures { spec, positions })
}
