//! Basic strategies consume only player-authorized observations and question data.
//!
//! Selection helpers do not read Game, authority state, object identities or question tokens.
//! Their answers still require native validation by the player submission interface.

mod basic;
mod decision;
mod selection;

pub use basic::{BasicPolicy, Choice};
pub use selection::{SelectionAnswer, distribute, search, select};

/// Failure to construct a complete answer, distinct from an engine fault or game result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PolicyFailure {
    /// The public question has inconsistent bounds or lacks required metadata.
    #[error("invalid policy question")]
    InvalidQuestion,
    /// Every feasible selection was checked without finding a complete answer.
    #[error("no complete answer")]
    NoSolution,
    /// The deterministic search budget ran out; feasibility remains unknown.
    #[error("selection search exhausted")]
    SearchExhausted,
}
