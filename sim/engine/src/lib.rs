//! A data-driven, deterministic rules prototype with serializable resolution frames.

#![expect(
    clippy::multiple_crate_versions,
    reason = "jsonschema strum requires syn 2 while serde and thiserror require syn 3."
)]

extern crate alloc;

use core::fmt::Display;
use core::result;

#[cfg(feature = "runner")]
pub mod adapter;
pub mod ai;
pub mod assist;
pub mod catalog;
pub mod game;
pub mod ir;
pub mod policy;
mod random;
pub mod replay;

/// Authority-only diagnostics; player interfaces return reason-free submission results.
/// Classify the failed contract, never words in the diagnostic message.
#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineFailure {
    /// A formal play cannot complete its mandatory targets or full payment.
    #[error("illegal play: {0}")]
    IllegalPlay(String),
    /// A deliberately unimplemented language or rule feature.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// Malformed data, a damaged save, or an invalid transport request.
    #[error("invalid: {0}")]
    Invalid(String),
    /// A runtime read or reference lacks the required information authority.
    #[error("authorization integrity: {0}")]
    AuthorizationIntegrity(String),
    /// An internal invariant or the engine's numeric representation was exceeded.
    #[error("invariant: {0}")]
    Invariant(String),
}

/// Engine fallible operations.
pub type Result<T> = result::Result<T, EngineFailure>;

fn invalid(error: impl Display) -> EngineFailure {
    EngineFailure::Invalid(error.to_string())
}
