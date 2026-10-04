//! clap owns parsing while positional defaults preserve the evaluation interface.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "sve-prototype",
    version,
    about = "Run reproducible engine evaluations"
)]
pub(crate) struct Cli {
    /// Card database snapshot in JSONL format.
    #[arg(value_name = "SNAPSHOT")]
    pub(crate) snapshot: PathBuf,
    /// Project root containing authored data and scenario fixtures.
    #[arg(value_name = "ROOT", default_value = ".")]
    pub(crate) root: PathBuf,
    /// Evaluation to run.
    #[arg(value_name = "MODE", value_enum, default_value = "all")]
    pub(crate) mode: Mode,
    /// Directory for generated reports.
    #[arg(value_name = "OUTPUT", default_value = "target/prototype")]
    pub(crate) output: PathBuf,
    /// Selection file for rules or known-failure list for gate.
    #[arg(value_name = "SELECTION_OR_KNOWN")]
    pub(crate) selection_or_known: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum Mode {
    All,
    G1,
    Ai,
    Replay,
    Assist,
    Validate,
    Rules,
    Gate,
}

#[cfg(test)]
mod tests;
