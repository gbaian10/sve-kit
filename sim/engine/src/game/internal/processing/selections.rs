//! Retrying formal selections preserves the earlier mode and cost declarations.

use super::{Execution, PlayDeclaration};

pub(in crate::game::internal) fn record_play_declaration(
    execution: &mut Execution,
    declaration: PlayDeclaration,
) {
    execution.play_declarations.push(declaration);
}

pub(in crate::game::internal) fn discard_selection_round(execution: &mut Execution) {
    execution.target_declarations.clear();
    execution
        .play_declarations
        .retain(|declaration| match declaration {
            PlayDeclaration::Recipients { .. }
            | PlayDeclaration::Materials { .. }
            | PlayDeclaration::Distribution { .. } => false,
            PlayDeclaration::X { .. }
            | PlayDeclaration::Modes { .. }
            | PlayDeclaration::OptionalCost { .. } => true,
        });
}
