//! Shared AI positions of phase D: same position, same visible information, same budget.
//!
//! Both designers wrap their AI prototype in [`AiEngine`]; [`check_ai`] runs the
//! positions in `tests/ai-positions/` and reports each check (H1–H3, Q0–Q3, R0–R4,
//! P0–P2) on its own. Hard checks cover legality, complete legal sets, not cheating and
//! profiles; the AI's self-reported search `trace` is only a diagnostic.
//! [`check_search_log`] is the audit step Q4, run on an engine-written search log.

mod checks;
mod search_log;

use alloc::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::ScenarioError;
use crate::engine::{EngineError, Step, View};
use crate::inherit::Fixture;
use crate::model::from_yaml;

pub use checks::check_ai;
pub use search_log::check_search_log;

/// Seed passed to `load`; randomness that the positions care about is controlled by `random`.
pub const LOAD_SEED: &str = "sve-ai-positions";

/// What the AI returns for one `think` call.
#[derive(Debug, Clone, PartialEq)]
pub struct AiReport {
    /// The chosen decision, in contract format.
    pub decision: Value,
    /// Best candidates with their scores (diagnostic).
    pub candidates: Vec<(Value, f64)>,
    /// Self-reported search trace `{depth, by, at, options}` (diagnostic only).
    pub trace: Vec<Value>,
    /// Player-decision edges explored.
    pub edges: u64,
    /// Engine-internal steps (reported).
    pub engine_steps: u64,
    /// Wall time (reported).
    pub millis: u64,
}

/// An AI prototype, wrapped by its adapter.
pub trait AiEngine {
    /// Builds the position; `setup.room` carries the position's `room`, if any.
    ///
    /// # Errors
    /// When the adapter cannot build the position.
    fn load(&mut self, fixture: &Fixture, seed: &str) -> Result<(), EngineError>;
    /// Submits one decision, as the runner's `Engine::decide`.
    ///
    /// # Errors
    /// When the adapter cannot submit it.
    fn decide(&mut self, decision: &Value) -> Result<Step, EngineError>;
    /// The complete legal set at the current input point, before any AI pruning.
    ///
    /// # Errors
    /// When the state cannot be read.
    fn legal(&self) -> Result<Vec<Value>, EngineError>;
    /// That seat's payload (`Omniscient` for the third party only).
    ///
    /// # Errors
    /// When the state cannot be read.
    fn projection(&self, view: View) -> Result<Value, EngineError>;
    /// An `assert` path as in the runner.
    ///
    /// # Errors
    /// When the state cannot be read.
    fn query(&self, view: View, path: &str) -> Result<Option<Value>, EngineError>;
    /// The current input point as `{by, at}` (contract section 4), or `None` when the
    /// game has ended.
    ///
    /// # Errors
    /// When the state cannot be read.
    fn awaiting(&self) -> Result<Option<Value>, EngineError>;
    /// Lets the AI in `seat` pick a decision; must not change the current position.
    ///
    /// # Errors
    /// When the AI cannot run.
    fn think(
        &mut self,
        seat: View,
        profile: &str,
        budget: u64,
        seed: &str,
    ) -> Result<AiReport, EngineError>;
}

/// A fresh engine per call: every check starts from a newly loaded position.
pub type AiFactory<'factory> = dyn FnMut() -> Box<dyn AiEngine> + 'factory;

/// Result of one check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiOutcome {
    /// The check holds.
    Pass,
    /// The check does not hold; one line per mismatch.
    Fail {
        /// What did not match.
        reasons: Vec<String>,
    },
    /// Reported only, never a failure.
    Diagnostic {
        /// What was observed.
        notes: Vec<String>,
    },
    /// The prototype does not implement something the check needs.
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

/// One reported check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCheckReport {
    /// Position id, e.g. `ai-q-a`.
    pub position: String,
    /// Check id, e.g. `Q2`.
    pub id: String,
    /// The result.
    pub outcome: AiOutcome,
}

/// One position file.
#[derive(Debug, Clone, Deserialize)]
pub struct AiPosition {
    /// `sve-ai-position/1`.
    pub schema: String,
    /// Position id.
    pub id: String,
    /// Room settings, e.g. `{open_decklists: true}`.
    #[serde(default)]
    pub room: Option<Value>,
    /// Setup in contract format.
    pub setup: Value,
    /// Controlled randomness for the decisions this file submits.
    #[serde(default)]
    pub random: Value,
    /// File stem of the paired position.
    #[serde(default)]
    pub pair: Option<String>,
    /// P2's Quick cards and their cost, for the Q4 audit.
    #[serde(default)]
    pub quick_cards: Vec<QuickCard>,
    /// The checks.
    pub checks: Vec<Check>,
}

/// A Quick card of the public deck list.
#[derive(Debug, Clone, Deserialize)]
pub struct QuickCard {
    /// Card number.
    pub card: String,
    /// Play cost in PP.
    pub cost: u64,
}

/// Decisions to submit first, or the id of a check whose `before` to reuse.
#[derive(Debug, Clone)]
pub enum Before {
    /// Decisions in contract format.
    Decisions(Vec<Value>),
    /// Another check's id.
    Ref(String),
}

/// One `think` call.
#[derive(Debug, Clone, Deserialize)]
pub struct Think {
    /// `P1` or `P2`.
    pub seat: String,
    /// Profile id.
    pub profile: String,
    /// Player-decision edges.
    pub budget: u64,
    /// AI seed.
    pub seed: String,
}

/// A `think` call and the decision it must return.
#[derive(Debug, Clone, Deserialize)]
pub struct ThinkExpect {
    /// The call.
    pub think: Think,
    /// The expected decision.
    pub decision: Value,
}

/// A `query` from one view.
#[derive(Debug, Clone, Deserialize)]
pub struct Query {
    /// `P1`, `P2` or `omniscient`.
    pub view: String,
    /// The path.
    pub path: String,
}

/// A branch of a check: starts from the parent's state.
#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    /// Name, for reports.
    pub name: String,
    /// Decisions after the parent's.
    #[serde(default)]
    pub before: Vec<Value>,
    /// Controlled randomness applied after the parent's.
    #[serde(default)]
    pub random: Option<Value>,
    /// Expected outcome of each of this branch's own `before` decisions.
    #[serde(default)]
    pub outcomes: Vec<String>,
    /// `{by, at}` of the input point where `legal_exact` is compared.
    #[serde(default)]
    pub awaiting: Option<Value>,
    /// The legal set at this point.
    #[serde(default)]
    pub legal_exact: Option<Vec<Value>>,
    /// Nested branches.
    #[serde(default)]
    pub branches: Vec<Self>,
}

/// One check of a position.
#[derive(Debug, Clone, Deserialize)]
pub struct Check {
    /// Check id.
    pub id: String,
    /// `root`, when no decision comes first.
    #[serde(default)]
    pub at: Option<String>,
    /// Decisions to submit first (a list), or another check's id (a string).
    #[serde(default)]
    pub before: Option<Value>,
    /// Branches continuing from this check's state.
    #[serde(default)]
    pub branches: Vec<Branch>,
    /// Expected outcome of each `before` decision (taken from the referenced check for a reference).
    #[serde(default)]
    pub outcomes: Vec<String>,
    /// `{by, at}` of the input point where `legal_exact` is compared.
    #[serde(default)]
    pub awaiting: Option<Value>,
    /// The legal set equals this list.
    #[serde(default)]
    pub legal_exact: Option<Vec<Value>>,
    /// A `think` call.
    #[serde(default)]
    pub think: Option<Think>,
    /// The decision belongs to that check's `legal_exact`.
    #[serde(default)]
    pub decision_in: Option<String>,
    /// These report fields equal the paired position's.
    #[serde(default)]
    pub equal_to_pair: Option<Vec<String>>,
    /// That seat's projection equals the paired position's.
    #[serde(default)]
    pub projection_equal_to_pair: Option<String>,
    /// A query whose value is checked.
    #[serde(default)]
    pub query: Option<Query>,
    /// The query equals this part of the setup.
    #[serde(default)]
    pub equals_setup: Option<String>,
    /// That seat's projection hides `objects`.
    #[serde(default)]
    pub projection_hides_from: Option<String>,
    /// Hidden objects.
    #[serde(default)]
    pub objects: Vec<String>,
    /// Several `think` calls on one instance.
    #[serde(default)]
    pub same_instance: Vec<ThinkExpect>,
    /// `omniscient`: the position must not change across each `think`.
    #[serde(default)]
    pub unchanged: Option<String>,
}

impl Check {
    /// This check's `before`, read as decisions or as a reference.
    #[must_use]
    pub fn before(&self) -> Option<Before> {
        match self.before.as_ref()? {
            Value::String(id) => Some(Before::Ref(id.clone())),
            Value::Array(list) => Some(Before::Decisions(list.clone())),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_) => None,
        }
    }
}

/// Every position, by file stem (`h`, `q-a`, …).
#[derive(Debug, Clone)]
pub struct AiPositions(pub BTreeMap<String, AiPosition>);

/// One entry of `profiles.yaml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ProfileEntry {
    /// `general`, `aggro` or `control`.
    #[serde(default)]
    pub id: Option<String>,
    /// Deck subtype.
    #[serde(default)]
    pub subtype: Option<String>,
    /// Version or period.
    #[serde(default)]
    pub period: Option<String>,
    /// Profile file, relative to `profiles.yaml`.
    #[serde(default)]
    pub file: Option<String>,
}

/// Loads every `*.yaml` position in a directory.
///
/// # Errors
/// When a file cannot be read or does not have the expected shape.
pub fn load_positions(dir: &Path) -> Result<AiPositions, ScenarioError> {
    let entries =
        fs::read_dir(dir).map_err(|e| ScenarioError::Io(format!("{}: {e}", dir.display())))?;
    let mut out = BTreeMap::new();
    for path in entries.filter_map(Result::ok).map(|e| e.path()) {
        if path.extension().is_none_or(|x| x != "yaml") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_owned();
        let text = fs::read_to_string(&path)
            .map_err(|e| ScenarioError::Io(format!("{}: {e}", path.display())))?;
        let position: AiPosition = from_yaml(&text)
            .map_err(|e| ScenarioError::Question(format!("{}: {e}", path.display())))?;
        out.insert(stem, position);
    }
    Ok(AiPositions(out))
}
