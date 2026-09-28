//! Engine-neutral runner for the rules scenarios in `tests/rules-scenarios/`.
//!
//! An engine under test implements [`Engine`] through a thin adapter; [`run`] loads
//! each scenario, submits its decisions and compares every checkpoint as
//! `tests/rules-scenarios/CONTRACT.md` specifies. Rule failures, unsupported
//! features and adapter errors are reported separately.

extern crate alloc;

pub mod ai;
pub mod arch;
pub mod compare;
pub mod engine;
pub mod gate;
pub mod inherit;
pub mod model;
pub mod runner;

pub use engine::{Engine, EngineError, Step, View};
pub use gate::{GateProblem, GateReport, KnownFailures, gate, load_known_failures};
pub use inherit::Fixture;
pub use model::{Question, parse_question};
pub use runner::{
    RunOptions, Selection, Verdict, load_dir, load_selection, parse_selection, run, run_one,
    score_g1, summary,
};

/// Errors in the questions themselves or in reading them.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ScenarioError {
    /// A question file is malformed.
    #[error("question: {0}")]
    Question(String),
    /// A file could not be read.
    #[error("io: {0}")]
    Io(String),
    /// A selection does not match the questions.
    #[error("selection: {0}")]
    Selection(String),
}
