//! Authority-side projection into the shared battle contract.
//!
//! Native coordinates stay in the projector. Hosts bind each projection to its
//! authenticated seat and current question before submitting the decoded answer.

mod costs;
mod effects;
mod events;
mod history;
mod objects;
mod pending;
mod references;
mod selection;
mod sources;
mod turn;

pub use costs::project_costs;
pub use effects::{project_card_effects, project_player_effects};
pub use events::{EventFeed, EventObjects, FeedUpdate, ProjectedEvent, project_batch};
pub use history::project_history;
pub use objects::ObjectTable;
pub(crate) use objects::build_objects;
pub use pending::project_pending;
pub use references::{BattleReferences, PrintingFaceIdentity, ReferenceInput};
pub use selection::{CandidateDetails, SelectionProjection};
pub(crate) use turn::project_turn;

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
