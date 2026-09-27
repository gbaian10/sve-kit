//! Strict regression gate: every scenario must pass unless it is listed, with its
//! exact verdict, in a reviewed known-failure file.
//!
//! A listed scenario that starts passing also fails the gate, so the list can only
//! shrink on purpose and never hides a fixed or a changed failure.

use alloc::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::ScenarioError;
use crate::model::from_yaml;
use crate::runner::{ScenarioReport, Verdict};

/// The reviewed list of scenarios that are allowed to not pass.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownFailures {
    /// How many scenarios the full run must produce; guards against silently
    /// dropping questions.
    pub expected_scenarios: usize,
    /// Every scenario allowed to not pass.
    #[serde(default)]
    pub known: Vec<KnownFailure>,
}

/// One scenario allowed to not pass.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownFailure {
    /// Question id.
    pub question: String,
    /// Scenario name.
    pub scenario: String,
    /// The exact verdict kind it currently has: `fail`, `unsupported`,
    /// `adapter-error` or `ineligible`.
    pub status: String,
    /// The known-error entry that explains it.
    pub error: String,
}

/// Why the gate rejected a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateProblem {
    /// The run did not produce the expected number of scenarios.
    Count {
        /// From the known-failure file.
        expected: usize,
        /// Produced by the run.
        actual: usize,
    },
    /// A scenario that is not listed did not pass.
    Unexpected {
        /// Question id.
        question: String,
        /// Scenario name.
        scenario: String,
        /// Verdict kind.
        status: String,
    },
    /// A listed scenario now passes: remove it from the list.
    NowPasses {
        /// Question id.
        question: String,
        /// Scenario name.
        scenario: String,
    },
    /// A listed scenario fails in a different way than recorded.
    StatusChanged {
        /// Question id.
        question: String,
        /// Scenario name.
        scenario: String,
        /// Recorded verdict kind.
        listed: String,
        /// Actual verdict kind.
        actual: String,
    },
    /// A listed scenario does not exist in the run.
    Stale {
        /// Question id.
        question: String,
        /// Scenario name.
        scenario: String,
    },
    /// The run produced the same scenario twice (e.g. two question files share an id).
    DuplicateRun {
        /// Question id.
        question: String,
        /// Scenario name.
        scenario: String,
    },
    /// A listed status is not one of the verdict kinds.
    UnknownStatus {
        /// Question id.
        question: String,
        /// Scenario name.
        scenario: String,
        /// The listed status.
        status: String,
    },
    /// The same scenario is listed twice.
    Duplicate {
        /// Question id.
        question: String,
        /// Scenario name.
        scenario: String,
    },
}

/// Outcome of the gate over one full run.
#[derive(Debug, Clone, Default)]
pub struct GateReport {
    /// Scenarios that passed.
    pub passed: usize,
    /// Listed scenarios that still fail exactly as recorded.
    pub known: usize,
    /// Everything that makes the gate fail.
    pub problems: Vec<GateProblem>,
}

impl GateReport {
    /// True when the run matches the list exactly.
    #[must_use]
    pub const fn ok(&self) -> bool {
        self.problems.is_empty()
    }
}

/// The verdict kind as spelled in reports and in the known-failure file.
#[must_use]
pub const fn status(verdict: &Verdict) -> &'static str {
    match verdict {
        Verdict::Pass => "pass",
        Verdict::Fail { .. } => "fail",
        Verdict::Unsupported { .. } => "unsupported",
        Verdict::AdapterError { .. } => "adapter-error",
        Verdict::Ineligible { .. } => "ineligible",
    }
}

/// Reads a known-failure file.
///
/// # Errors
/// When the file cannot be read or has unknown fields.
pub fn load_known_failures(path: &Path) -> Result<KnownFailures, ScenarioError> {
    let text = fs::read_to_string(path)
        .map_err(|e| ScenarioError::Io(format!("{}: {e}", path.display())))?;
    from_yaml(&text).map_err(|e| ScenarioError::Selection(format!("{}: {e}", path.display())))
}

/// Compares a full run with the known-failure list.
#[must_use]
pub fn gate(reports: &[ScenarioReport], known: &KnownFailures) -> GateReport {
    let mut report = GateReport::default();
    let mut listed: BTreeMap<(&str, &str), &KnownFailure> = BTreeMap::new();
    for entry in &known.known {
        let key = (entry.question.as_str(), entry.scenario.as_str());
        if !matches!(
            entry.status.as_str(),
            "fail" | "unsupported" | "adapter-error" | "ineligible"
        ) {
            report.problems.push(GateProblem::UnknownStatus {
                question: entry.question.clone(),
                scenario: entry.scenario.clone(),
                status: entry.status.clone(),
            });
        }
        if listed.insert(key, entry).is_some() {
            report.problems.push(GateProblem::Duplicate {
                question: entry.question.clone(),
                scenario: entry.scenario.clone(),
            });
        }
    }
    if reports.len() != known.expected_scenarios {
        report.problems.push(GateProblem::Count {
            expected: known.expected_scenarios,
            actual: reports.len(),
        });
    }
    let mut seen = BTreeSet::new();
    for scenario in reports {
        let key = (scenario.question.as_str(), scenario.scenario.as_str());
        if !seen.insert(key) {
            report.problems.push(GateProblem::DuplicateRun {
                question: scenario.question.clone(),
                scenario: scenario.scenario.clone(),
            });
        }
        let actual = status(&scenario.verdict);
        match (listed.remove(&key), actual) {
            (None, "pass") => report.passed = report.passed.saturating_add(1),
            (None, _) => report.problems.push(GateProblem::Unexpected {
                question: scenario.question.clone(),
                scenario: scenario.scenario.clone(),
                status: actual.to_owned(),
            }),
            (Some(_), "pass") => {
                report.passed = report.passed.saturating_add(1);
                report.problems.push(GateProblem::NowPasses {
                    question: scenario.question.clone(),
                    scenario: scenario.scenario.clone(),
                });
            }
            (Some(entry), _) if entry.status != actual => {
                report.problems.push(GateProblem::StatusChanged {
                    question: scenario.question.clone(),
                    scenario: scenario.scenario.clone(),
                    listed: entry.status.clone(),
                    actual: actual.to_owned(),
                });
            }
            (Some(_), _) => report.known = report.known.saturating_add(1),
        }
    }
    for ((question, scenario), _) in listed {
        report.problems.push(GateProblem::Stale {
            question: question.to_owned(),
            scenario: scenario.to_owned(),
        });
    }
    report
}
