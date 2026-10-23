//! Internal batch selection; current programs remain candidates pending runtime admission.

use super::{Catalog, EngineFailure, Result, invalid};
use crate::ir::lowering::{CurrentCardProgram, ExpandedLowerer};
use alloc::collections::{BTreeMap, BTreeSet};
use serde::Deserialize;
use serde_json::Value;

/// Disjoint prototype programs and complete, current-source checked D0 candidates.
#[derive(Debug)]
pub struct SelectedEffects {
    /// Existing prototype runtime, without any switched card's prototype program.
    pub prototypes: Catalog,
    /// Whole-card candidates; selection does not grant execution or use qualification.
    pub current: BTreeMap<String, CurrentCardProgram>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    documents: Vec<(String, String)>,
    authors: Vec<Author>,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum Author {
    Accepted { input: Value },
    Rejected { card_no: String, reason: String },
}

/// Consume `CurrentAuthorSources.selection_input()` from the trusted Python reader.
///
/// The reader owns index closure and permanent-identity overlap checks. This
/// handoff is internal and deliberately not a serialized catalog/save format.
///
/// # Errors
/// Rejects the entire selection on any rejected card, overlap or D0 failure;
/// callers never receive a partial selection or an implicit prototype fallback.
pub(super) fn select(snapshot: &str, keywords: &str, handoff: &Value) -> Result<SelectedEffects> {
    let selection: Selection = serde_json::from_value(handoff.clone())
        .map_err(|_private| invalid("invalid current selection"))?;
    let prototypes = Catalog::from_documents(snapshot, keywords, &selection.documents)?;
    let lowerer = ExpandedLowerer::new().map_err(invalid)?;
    let mut current = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for author in selection.authors {
        let candidate = match author {
            Author::Accepted { input } => input,
            Author::Rejected { card_no, reason } => {
                return Err(EngineFailure::Unsupported(format!(
                    "current card rejected: {card_no}: {reason}"
                )));
            }
        };
        let program = lowerer.lower_current(&candidate).map_err(invalid)?;
        let number = program.card_no.clone();
        if !prototypes.cards.contains_key(&number)
            || prototypes.programs.contains_key(&number)
            || prototypes.rejected.contains_key(&number)
            || !identities.insert(program.card_id.clone())
            || current.insert(number, program).is_some()
        {
            return Err(invalid("current selection identity or prototype overlap"));
        }
    }
    Ok(SelectedEffects {
        prototypes,
        current,
    })
}
