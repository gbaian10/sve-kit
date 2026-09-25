//! Question files as the runner sees them (contract `sve-exam/2.1`).
//!
//! Only the parts the runner itself interprets are typed. `setup`, `card_facts`,
//! `random` and each decision stay as neutral JSON values: turning them into engine
//! state is the adapter's job, so the runner never imposes an internal representation.

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::Error;

/// One question file.
#[derive(Debug, Clone, Deserialize)]
pub struct Question {
    /// `card-<number>`, `rule-<clause>-<n>` or `flow-<topic>-<n>`.
    pub id: String,
    /// Whether the question counts toward the must-pass bar.
    #[serde(default)]
    pub must_pass: bool,
    /// `draft`, `verified` or `disputed`.
    #[serde(default)]
    pub status: String,
    /// The scenarios, each independent.
    pub scenarios: Vec<Scenario>,
}

/// One scenario as written in the file, before `inherit` is expanded.
#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    /// Unique within the question.
    pub name: String,
    /// Name of the scenario whose `setup` and `card_facts` this one starts from.
    #[serde(default)]
    pub inherit: Option<String>,
    /// Neutral description of the position.
    #[serde(default)]
    pub setup: Option<Value>,
    /// Per-card-number fact patches.
    #[serde(default)]
    pub card_facts: Option<Value>,
    /// Player decisions in order; `n` counts from 1.
    #[serde(default)]
    pub decisions: Vec<Value>,
    /// Controlled randomness: shuffle results, random selections, dice.
    #[serde(default)]
    pub random: Option<Value>,
    /// Checkpoints to compare.
    #[serde(default)]
    pub expected: Vec<Expected>,
}

/// One expected checkpoint entry (contract section 6).
#[derive(Debug, Clone, Deserialize)]
pub struct Expected {
    /// `after-decision-N`.
    pub at: String,
    /// `omniscient`, `P1` or `P2`.
    pub view: String,
    /// Expected outcome of decision N.
    pub outcome: String,
    /// Path → expected value; only written keys are compared.
    #[serde(default)]
    pub assert: Map<String, Value>,
    /// Events that must appear in order (subsequence).
    #[serde(default)]
    pub events: Vec<Value>,
    /// Event kinds whose occurrences must equal the listed `events` exactly.
    #[serde(default)]
    pub events_exact: Vec<String>,
    /// Events that must not appear.
    #[serde(default)]
    pub forbidden_events: Vec<Value>,
    /// The decision the engine must be waiting for.
    #[serde(default)]
    pub awaiting: Option<Value>,
}

impl Expected {
    /// The decision number this checkpoint follows.
    ///
    /// # Errors
    /// When `at` is not of the form `after-decision-N`.
    pub fn decision_index(&self) -> Result<usize, Error> {
        self.at
            .strip_prefix("after-decision-")
            .and_then(|n| n.parse().ok())
            .ok_or_else(|| Error::Question(format!("bad checkpoint {:?}", self.at)))
    }
}

/// Parses one question file.
///
/// # Errors
/// When the text is not valid YAML or does not have the question shape.
pub fn parse_question(text: &str) -> Result<Question, Error> {
    from_yaml(text).map_err(|e| Error::Question(e.to_string()))
}

/// The one YAML reader for question and selection files.
///
/// Booleans are YAML 1.2 (`true`/`false` only): the question files are also read by
/// PyYAML-based tools, and YAML 1.1 forms such as `y`, `n`, `on` or `off` would
/// otherwise turn object ids or card fields into booleans. Duplicate keys are errors.
///
/// # Errors
/// When the text is not valid YAML or does not deserialize into `T`.
pub fn from_yaml<'de, T: Deserialize<'de>>(text: &'de str) -> Result<T, serde_saphyr::Error> {
    serde_saphyr::from_str_with_options(text, serde_saphyr::options! { strict_booleans: true })
}
