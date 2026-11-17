//! A known source does not disclose a frame; actual question publication supplies that witness.

use super::{Execution, Game};
use crate::game::internal::{fact_log::viewer, observation::EffectOrigin};
use crate::game::player::Viewer;
use crate::game::player::observation::ProcessingSource;
use alloc::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub(in crate::game::internal) struct ProcessingDisclosure {
    sources: BTreeMap<Viewer, EffectOrigin>,
}

// Publication callers supply the witness; affordability probes can build identical waits.
pub(in crate::game::internal) fn disclose_source(game: &Game, execution: &mut Execution) {
    if execution.play_phase.is_none() {
        return;
    }
    let Some((question, _)) = &execution.waiting else {
        return;
    };
    let Some(actor) = viewer(question.actor) else {
        return;
    };
    let Some(information) = &execution.source_information else {
        return;
    };
    let origin = information.origin();
    if origin.observed(game, actor).is_some() {
        // Preserve the first authorized origin, independently of later holder moves or text changes.
        execution
            .processing_disclosure
            .sources
            .entry(actor)
            .or_insert_with(|| origin.clone());
    }
}

pub(in crate::game) fn observed_sources(game: &Game, perspective: Viewer) -> Vec<ProcessingSource> {
    if game.player_fault.is_some() || game.state.game["ended"] == true {
        return vec![];
    }
    let mut sources = vec![];
    if let Some(execution) = &game.typed {
        append_sources(game, execution, perspective, &mut sources);
    }
    sources
}

fn append_sources(
    game: &Game,
    execution: &Execution,
    perspective: Viewer,
    sources: &mut Vec<ProcessingSource>,
) {
    if let Some(nested) = &execution.nested {
        append_sources(game, &nested.parent, perspective, sources);
    }
    if let Some(origin) = execution.processing_disclosure.sources.get(&perspective)
        && let Some(controller) = viewer(execution.context.controller)
        && let Some(source) = origin.observed(game, perspective)
    {
        sources.push(ProcessingSource { controller, source });
    }
}
