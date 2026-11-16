//! Rule-owned decisions are separate from effect answer domains.

use super::{Binding, Result};
use crate::battle::ProjectionError;
use crate::game::player;
use sve_battle_contract::{observation::ProcessingStage, question as wire};

pub(super) const fn context() -> Result<(
    player::QuestionPurpose,
    wire::QuestionPurpose,
    ProcessingStage,
)> {
    // A flow stop needs its authoritative rules purpose and processing context together.
    Err(ProjectionError::Unavailable)
}

pub(super) const fn project() -> Result<(wire::QuestionUse, wire::QuestionShape, Binding)> {
    // Native flow contexts and occurrence sources must exist before these stops can be answered.
    Err(ProjectionError::Unavailable)
}
