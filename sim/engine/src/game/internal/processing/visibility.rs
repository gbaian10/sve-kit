//! A known source needs a frame witness: actual question publication or its public played writer.

use super::{Execution, Game};
use crate::game::internal::{
    fact_log::viewer, observation::EffectOrigin, source::SourceInformation,
};
use crate::game::player::Viewer;
use crate::game::player::observation::ProcessingSource;
use alloc::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub(in crate::game::internal) struct ProcessingDisclosure {
    sources: BTreeMap<Viewer, EffectOrigin>,
}

// execute_internal, answer_internal and card_play_result publish real waits; probes do not.
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
    disclose_to(game, execution, [actor]);
}

pub(in crate::game::internal) fn disclose_played_source(game: &Game, execution: &mut Execution) {
    // The played writer establishes existence; a private holder still cannot disclose its text.
    if execution
        .source_information
        .as_ref()
        .is_some_and(SourceInformation::publicly_identified)
    {
        disclose_to(game, execution, [Viewer::P1, Viewer::P2]);
    }
}

fn disclose_to(game: &Game, execution: &mut Execution, audience: impl IntoIterator<Item = Viewer>) {
    let Some(information) = &execution.source_information else {
        return;
    };
    let origin = information.origin();
    for actor in audience {
        if origin.observed(game, actor).is_none() {
            continue;
        }
        // Preserve the first authorized origin, independently of later holder moves or text changes.
        execution
            .processing_disclosure
            .sources
            .entry(actor)
            .or_insert_with(|| origin.clone());
    }
}

pub(in crate::game) fn observed_sources(game: &Game, perspective: Viewer) -> Vec<ProcessingSource> {
    // Sources belong to retained typed state, whether a fault restores a stop or keeps it as-is.
    if game.state.game["ended"] == true {
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
