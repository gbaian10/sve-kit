//! Accepted play targets belong to their execution, not to a mutable binding slot.

use crate::ir::{DecisionId, ObjectRef, PlayerId, TargetRoleAnswer};
use alloc::vec::Vec;

/// One explicitly accepted formal-play selection, not a resolution choice or authorization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetDeclaration {
    /// Actual answering player, distinct from the execution controller.
    pub player: PlayerId,
    /// The admitted play parameter whose answer was accepted.
    pub decision: DecisionId,
    /// Selected physical generations in accepted answer order; these may be private.
    pub objects: Vec<ObjectRef>,
    /// Validated role assignments, with physical generations rather than question handles.
    pub roles: Vec<TargetRoleAnswer>,
}

pub(in crate::game::internal) fn record_target_declaration(
    execution: &mut super::Execution,
    declaration: TargetDeclaration,
) {
    execution.target_declarations.push(declaration);
}
