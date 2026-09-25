//! Runs scenarios against an engine and reports rule failures apart from
//! adapter failures and unsupported features.

use alloc::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ScenarioError;
use crate::compare;
use crate::engine::{Engine, EngineError, View};
use crate::inherit::{Fixture, expand};
use crate::model::{Expected, Question, from_yaml, parse_question};

/// Result of one scenario.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Verdict {
    /// Every checkpoint matched.
    Pass,
    /// At least one checkpoint did not match.
    Fail {
        /// Every mismatch found, in checkpoint order.
        failures: Vec<Failure>,
    },
    /// The engine does not implement something the scenario needs.
    Unsupported {
        /// Engine's explanation.
        reason: String,
    },
    /// The adapter failed; not counted as a rule failure.
    AdapterError {
        /// Adapter's explanation.
        reason: String,
    },
    /// Not run: the question may not count toward acceptance (contract 7).
    Ineligible {
        /// Why: not `verified`, or it relies on an unconfirmed card fact.
        reason: String,
    },
}

/// How to run.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    /// Score only questions that are `status: verified` and rely on no
    /// `card_facts` entry with `verified: false`; others become [`Verdict::Ineligible`].
    pub require_verified: bool,
}

/// One mismatch at a checkpoint.
#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    /// `after-decision-N`.
    pub at: String,
    /// The view the checkpoint was observed from.
    pub view: String,
    /// What was compared: `outcome`, `assert <path>`, `events`, `events_exact`,
    /// `forbidden_events`, `awaiting`.
    pub what: String,
    /// What the question expects.
    pub expected: Value,
    /// What the engine produced.
    pub actual: Value,
}

/// Result of one scenario, with its identity.
#[derive(Debug, Clone, Serialize)]
pub struct ScenarioReport {
    /// Question id.
    pub question: String,
    /// Scenario name.
    pub scenario: String,
    /// The verdict.
    #[serde(flatten)]
    pub verdict: Verdict,
}

/// Which scenarios to run: question id → `all` or a list of scenario names.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Selection(pub BTreeMap<String, Pick>);

/// Scenarios picked from one question.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Pick {
    /// The literal `all`.
    All(String),
    /// Named scenarios.
    Named(Vec<String>),
}

impl Selection {
    fn allows(&self, question: &str, scenario: &str) -> bool {
        match self.0.get(question) {
            None => false,
            Some(Pick::All(_)) => true,
            Some(Pick::Named(names)) => names.iter().any(|n| n == scenario),
        }
    }

    /// Checks every entry against the questions and returns how many scenarios it
    /// selects. A misspelt id or name is an error, never a silently skipped scenario.
    ///
    /// # Errors
    /// On an unknown question or scenario, a duplicate name, or a word other than `all`.
    pub fn validate(&self, questions: &[Question]) -> Result<usize, ScenarioError> {
        let mut total = 0_usize;
        for (id, pick) in &self.0 {
            let q = questions
                .iter()
                .find(|q| &q.id == id)
                .ok_or_else(|| ScenarioError::Selection(format!("unknown question {id:?}")))?;
            match pick {
                Pick::All(word) if word == "all" => total = total.saturating_add(q.scenarios.len()),
                Pick::All(word) => {
                    return Err(ScenarioError::Selection(format!(
                        "{id}: expected `all`, got {word:?}"
                    )));
                }
                Pick::Named(names) => {
                    for (i, name) in names.iter().enumerate() {
                        if names.iter().take(i).any(|n| n == name) {
                            return Err(ScenarioError::Selection(format!(
                                "{id}: duplicate {name:?}"
                            )));
                        }
                        if !q.scenarios.iter().any(|s| &s.name == name) {
                            return Err(ScenarioError::Selection(format!(
                                "{id}: unknown scenario {name:?}"
                            )));
                        }
                    }
                    total = total.saturating_add(names.len());
                }
            }
        }
        Ok(total)
    }
}

/// Reads a selection file: question id → `all` or a list of scenario names.
///
/// # Errors
/// When the file cannot be read or does not have that shape.
pub fn load_selection(path: &Path) -> Result<Selection, ScenarioError> {
    let text = fs::read_to_string(path)
        .map_err(|e| ScenarioError::Io(format!("{}: {e}", path.display())))?;
    parse_selection(&text).map_err(|e| ScenarioError::Selection(format!("{}: {e}", path.display())))
}

/// Parses a selection from text.
///
/// # Errors
/// When the text does not have the selection shape.
pub fn parse_selection(text: &str) -> Result<Selection, ScenarioError> {
    from_yaml(text).map_err(|e| ScenarioError::Selection(e.to_string()))
}

/// Loads every question file under a directory (sorted by file name).
///
/// # Errors
/// When a file cannot be read or parsed.
pub fn load_dir(dir: &Path) -> Result<Vec<Question>, ScenarioError> {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .map_err(|e| ScenarioError::Io(format!("{}: {e}", dir.display())))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let text = fs::read_to_string(p)
                .map_err(|e| ScenarioError::Io(format!("{}: {e}", p.display())))?;
            parse_question(&text)
                .map_err(|e| ScenarioError::Question(format!("{}: {e}", p.display())))
        })
        .collect()
}

/// Runs the selected scenarios (all of them when `selection` is `None`).
///
/// # Errors
/// When the selection does not match the questions, or a question itself is
/// malformed (bad `inherit`, a checkpoint outside its decisions).
pub fn run(
    engine: &mut dyn Engine,
    questions: &[Question],
    selection: Option<&Selection>,
    options: RunOptions,
) -> Result<Vec<ScenarioReport>, ScenarioError> {
    let wanted = selection.map(|s| s.validate(questions)).transpose()?;
    let mut reports = Vec::new();
    for q in questions {
        let fixtures = expand(&q.id, &q.scenarios)?;
        for (scenario, fixture) in q.scenarios.iter().zip(&fixtures) {
            if selection.is_some_and(|s| !s.allows(&q.id, &scenario.name)) {
                continue;
            }
            let verdict = match ineligible(q, fixture, options) {
                Some(reason) => Verdict::Ineligible { reason },
                None => run_one(engine, fixture, &scenario.decisions, &scenario.expected)?,
            };
            reports.push(ScenarioReport {
                question: q.id.clone(),
                scenario: scenario.name.clone(),
                verdict,
            });
        }
    }
    if let Some(n) = wanted
        && n != reports.len()
    {
        return Err(ScenarioError::Selection(format!(
            "selection names {n} scenarios but {} ran",
            reports.len()
        )));
    }
    Ok(reports)
}

/// The G1 scoring entry: fail closed on anything that would make the score unreliable.
///
/// Requires a non-empty selection of named scenarios (no `all`), exactly
/// `expected_scenarios` of them, and verified questions only. Any ineligible
/// scenario is an error, not a skip.
///
/// # Errors
/// When any of those conditions fails, or a question is malformed.
pub fn score_g1(
    engine: &mut dyn Engine,
    questions: &[Question],
    selection: &Selection,
    expected_scenarios: usize,
) -> Result<Vec<ScenarioReport>, ScenarioError> {
    if selection.0.values().any(|p| matches!(p, Pick::All(_))) {
        return Err(ScenarioError::Selection(
            "G1 selections must name every scenario".into(),
        ));
    }
    let named = selection.validate(questions)?;
    if named == 0 || named != expected_scenarios {
        return Err(ScenarioError::Selection(format!(
            "G1 expects {expected_scenarios} scenarios, the selection names {named}"
        )));
    }
    let options = RunOptions {
        require_verified: true,
    };
    let reports = run(engine, questions, Some(selection), options)?;
    if let Some(r) = reports
        .iter()
        .find(|r| matches!(r.verdict, Verdict::Ineligible { .. }))
    {
        return Err(ScenarioError::Selection(format!(
            "{} / {} is not eligible for scoring: {:?}",
            r.question, r.scenario, r.verdict
        )));
    }
    Ok(reports)
}

fn ineligible(q: &Question, fixture: &Fixture, options: RunOptions) -> Option<String> {
    if !options.require_verified {
        return None;
    }
    if q.status != "verified" {
        return Some(format!("status is {:?}", q.status));
    }
    let unconfirmed: Vec<&String> = fixture
        .card_facts
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, facts)| facts.get("verified") == Some(&Value::Bool(false)))
        .map(|(card, _)| card)
        .collect();
    (!unconfirmed.is_empty()).then(|| format!("unconfirmed card facts: {unconfirmed:?}"))
}

/// Runs one expanded scenario.
///
/// # Errors
/// When a checkpoint in the question is malformed.
pub fn run_one(
    engine: &mut dyn Engine,
    fixture: &Fixture,
    decisions: &[Value],
    expected: &[Expected],
) -> Result<Verdict, ScenarioError> {
    let mut checkpoints: BTreeMap<usize, Vec<&Expected>> = BTreeMap::new();
    for e in expected {
        let n = e.decision_index()?;
        if n == 0 || n > decisions.len() {
            return Err(ScenarioError::Question(format!(
                "{}: {} is outside its {} decisions",
                fixture.scenario,
                e.at,
                decisions.len()
            )));
        }
        checkpoints.entry(n).or_default().push(e);
    }
    if let Err(e) = engine.load(fixture) {
        return Ok(engine_verdict(e));
    }
    let mut failures = Vec::new();
    let mut window: Vec<Value> = Vec::new();
    for (i, decision) in decisions.iter().enumerate() {
        let n = i.saturating_add(1);
        let step = match engine.decide(decision) {
            Ok(step) => step,
            Err(e) => return Ok(engine_verdict(e)),
        };
        window.extend(step.events);
        let Some(entries) = checkpoints.get(&n) else {
            continue;
        };
        if let Some(label) = compare::broken_group(&window) {
            failures.push(Failure {
                at: format!("after-decision-{n}"),
                view: "omniscient".into(),
                what: "group not contiguous".into(),
                expected: Value::Null,
                actual: label,
            });
        }
        for entry in entries {
            if let Err(e) = check(
                engine,
                fixture,
                entry,
                &step.outcome,
                &window,
                &mut failures,
            ) {
                return Ok(engine_verdict(e));
            }
        }
        // The event range restarts after each distinct checkpoint (contract 6.4).
        window.clear();
    }
    Ok(if failures.is_empty() {
        Verdict::Pass
    } else {
        Verdict::Fail { failures }
    })
}

fn engine_verdict(e: EngineError) -> Verdict {
    match e {
        EngineError::Unsupported(reason) => Verdict::Unsupported { reason },
        EngineError::Adapter(reason) => Verdict::AdapterError { reason },
    }
}

fn check(
    engine: &dyn Engine,
    fixture: &Fixture,
    entry: &Expected,
    outcome: &str,
    events: &[Value],
    failures: &mut Vec<Failure>,
) -> Result<(), EngineError> {
    let view = View::parse(&entry.view)
        .ok_or_else(|| EngineError::Adapter(format!("unknown view {:?}", entry.view)))?;
    let mut fail = |what: String, expected: Value, actual: Value| {
        failures.push(Failure {
            at: entry.at.clone(),
            view: entry.view.clone(),
            what,
            expected,
            actual,
        });
    };
    if entry.outcome != outcome {
        fail(
            "outcome".into(),
            entry.outcome.clone().into(),
            outcome.into(),
        );
    }
    for (path, want) in &entry.assert {
        let got = engine.query(view, path)?;
        if !got
            .as_ref()
            .is_some_and(|g| compare::assert_value(path, want, g))
        {
            fail(
                format!("assert {path}"),
                want.clone(),
                got.unwrap_or(Value::Null),
            );
        }
    }
    if !compare::events_subsequence(&entry.events, events) {
        fail(
            "events".into(),
            entry.events.clone().into(),
            events.to_vec().into(),
        );
    }
    if !entry.events_exact.is_empty()
        && !compare::events_exact(&entry.events_exact, &entry.events, events)
    {
        fail(
            format!("events_exact {:?}", entry.events_exact),
            entry.events.clone().into(),
            events.to_vec().into(),
        );
    }
    if let Some(hit) = compare::forbidden_hit(&entry.forbidden_events, events) {
        fail(
            "forbidden_events".into(),
            hit.clone(),
            events.to_vec().into(),
        );
    }
    if let Some(want) = &entry.awaiting {
        let got = engine.awaiting(view)?;
        if !compare::awaiting(view, want, got.as_ref()) {
            fail("awaiting".into(), want.clone(), got.unwrap_or(Value::Null));
        }
    }
    if let Some(hidden) = entry
        .assert
        .get("knowledge")
        .and_then(|k| k.get("does_not_know"))
        .and_then(Value::as_array)
    {
        let leaked = leaks(engine, fixture, view, hidden)?;
        if !leaked.is_empty() {
            fail(
                "projection leak".into(),
                Value::Array(Vec::new()),
                leaked.into(),
            );
        }
    }
    Ok(())
}

/// Hidden objects the viewer's outputs give away: by id anywhere in the projection or
/// in `awaiting`, or by card number unless another card with that number is one the
/// viewer can legitimately identify.
fn leaks(
    engine: &dyn Engine,
    fixture: &Fixture,
    view: View,
    hidden: &[Value],
) -> Result<Vec<Value>, EngineError> {
    let outputs = Value::Array(vec![
        engine.projection(view)?,
        engine.awaiting(view)?.unwrap_or(Value::Null),
    ]);
    let cards = card_numbers(&fixture.setup);
    let identifiable = engine
        .query(view, "knowledge")?
        .and_then(|k| k.get("identifiable").and_then(Value::as_array).cloned())
        .unwrap_or_default();
    let public_cards: Vec<&str> = identifiable
        .iter()
        .filter_map(|id| id.as_str().and_then(|i| cards.get(i)).map(String::as_str))
        .collect();
    Ok(hidden
        .iter()
        .filter(|id| {
            compare::mentions(&outputs, id)
                || id.as_str().and_then(|i| cards.get(i)).is_some_and(|card| {
                    !public_cards.contains(&card.as_str()) && compare::contains_text(&outputs, card)
                })
        })
        .cloned()
        .collect())
}

/// Object id → card number, from every zone of the setup.
fn card_numbers(setup: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let players = setup.get("players").and_then(Value::as_object);
    for player in players.into_iter().flat_map(|p| p.values()) {
        let zones = player.get("zones").and_then(Value::as_object);
        let lists = ["deck_list", "evolve_deck_list"]
            .iter()
            .filter_map(|k| player.get(*k));
        for entry in zones
            .into_iter()
            .flat_map(|z| z.values())
            .chain(lists)
            .filter_map(Value::as_array)
            .flatten()
        {
            if let (Some(id), Some(card)) = (
                entry.get("id").and_then(Value::as_str),
                entry.get("card").and_then(Value::as_str),
            ) {
                out.insert(id.to_owned(), card.to_owned());
            }
        }
    }
    out
}

/// Counts per verdict, for the summary line.
#[must_use]
pub fn summary(reports: &[ScenarioReport]) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for r in reports {
        let key = match r.verdict {
            Verdict::Pass => "pass",
            Verdict::Fail { .. } => "fail",
            Verdict::Unsupported { .. } => "unsupported",
            Verdict::AdapterError { .. } => "adapter-error",
            Verdict::Ineligible { .. } => "ineligible",
        };
        let count = counts.entry(key).or_insert(0_usize);
        *count = count.saturating_add(1);
    }
    counts
}
