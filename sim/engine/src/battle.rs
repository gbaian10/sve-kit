//! Authority-side projection into the shared battle contract.
//!
//! Native coordinates stay in the projector. Hosts bind each projection to its
//! authenticated seat and current question before submitting the decoded answer.
//!
//! Board components stay private so hosts cannot replace
//! the authority's observation, even when the runner feature is enabled.
//!
//! ```compile_fail,E0432
//! use sve_engine::battle::project_pending;
//! ```
//!
//! ```compile_fail,E0432
//! use sve_engine::battle::project_card_effects;
//! ```
//!
//! ```compile_fail,E0432
//! use sve_engine::battle::project_player_effects;
//! ```
//!
//! ```compile_fail,E0432
//! use sve_engine::battle::project_costs;
//! ```
//!
//! ```compile_fail,E0432
//! use sve_engine::battle::project_history;
//! ```

mod board;
mod costs;
mod effects;
mod events;
mod history;
mod objects;
mod pending;
mod question;
mod references;
mod rules;
mod selection;
mod sources;
mod turn;
mod wording;

#[cfg(all(test, feature = "runner"))]
pub(crate) mod tests;

pub(crate) use board::{RuleProcessing, build_observation};
pub use events::{
    EventFeed, EventObjects, EventReferences, FeedUpdate, ProjectedEvent, project_batch,
};
pub use objects::ObjectTable;
pub(crate) use objects::build_objects;
pub use question::{AnswerError, QuestionProjection};
pub use references::{BattleReferences, PrintingFaceIdentity, ReferenceInput};
pub use rules::{RuleIdentity, RuleInput};
pub use selection::{CandidateDetails, SelectionProjection};
pub(crate) use turn::project_turn;
pub use wording::{
    WordingFaceIdentity, WordingFieldIdentity, WordingInput, WordingRevisionIdentity,
};

use sve_battle_contract::{HostError, HostErrorReason};

/// A required authority fact was unavailable; no partial interaction is returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProjectionError {
    /// A required authority fact cannot safely be represented.
    #[error("battle projection unavailable")]
    Unavailable,
    /// A native number exceeds the wire's exact integer range.
    #[error("battle projection number out of range")]
    Range,
}

impl ProjectionError {
    /// Safe host failure with no authority diagnostics or submitted payload.
    #[must_use]
    pub const fn host_error(self) -> HostError {
        match self {
            Self::Unavailable => HostError::PROJECTION,
            Self::Range => HostError::RANGE,
        }
    }
}

impl From<HostError> for ProjectionError {
    fn from(error: HostError) -> Self {
        match error.reason {
            HostErrorReason::Range => Self::Range,
            HostErrorReason::Format | HostErrorReason::Projection | HostErrorReason::Budget => {
                Self::Unavailable
            }
        }
    }
}
