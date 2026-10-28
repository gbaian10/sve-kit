//! Internal batch selection; current programs remain candidates pending runtime admission.

use super::{Catalog, EngineFailure, EngineIdentityInput, Result, invalid};
use crate::ir::lowering::{CurrentCardProgram, ExpandedLowerer};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use serde::Deserialize;
use serde_json::Value;

/// Disjoint prototype programs and complete, current-source checked D0 candidates.
#[derive(Debug)]
#[expect(
    clippy::partial_pub_fields,
    reason = "The public candidates are checked against private accepted copies before use."
)]
pub struct SelectedEffects {
    /// Existing prototype runtime, without any switched card's prototype program.
    pub prototypes: Arc<Catalog>,
    /// Whole-card candidates; selection does not grant execution or use qualification.
    pub current: BTreeMap<String, Arc<CurrentCardProgram>>,
    accepted: BTreeMap<String, Arc<CurrentCardProgram>>,
    accepted_prototypes: Arc<Catalog>,
}

impl SelectedEffects {
    pub(crate) fn check_runtime(&self) -> Result<()> {
        if !Arc::ptr_eq(&self.prototypes, &self.accepted_prototypes)
            || self.current.len() != self.accepted.len()
            || self.current.iter().any(|(number, candidate)| {
                self.accepted
                    .get(number)
                    .is_none_or(|accepted| !Arc::ptr_eq(candidate, accepted))
            })
        {
            return Err(invalid("current selection changed after acceptance"));
        }
        Ok(())
    }
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
pub(super) fn select(
    snapshot: &str,
    keywords: &str,
    handoff: &Value,
    rules: Option<(&str, &EngineIdentityInput)>,
) -> Result<SelectedEffects> {
    let selection: Selection = serde_json::from_value(handoff.clone())
        .map_err(|_private| invalid("invalid current selection"))?;
    let mut prototypes = Catalog::read_documents(snapshot, keywords, &selection.documents)?;
    let lowerer = ExpandedLowerer::new().map_err(invalid)?;
    let mut current = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for author in selection.authors {
        let candidate = match author {
            Author::Accepted { input } => input,
            Author::Rejected { card_no, reason } => {
                if reason == "source_unresolved_issue" {
                    if !prototypes.cards.contains_key(&card_no)
                        || prototypes.programs.contains_key(&card_no)
                        || prototypes.rejected.insert(card_no, vec![reason]).is_some()
                    {
                        return Err(invalid("unresolved current card overlaps a prototype"));
                    }
                    continue;
                }
                return Err(EngineFailure::Unsupported(format!(
                    "current card rejected: {card_no}: {reason}"
                )));
            }
        };
        let program = lowerer.lower_current(&candidate).map_err(invalid)?;
        let number = program.card_no.clone();
        let token = program.expanded.pointer(&format!("/cards/{number}/token"));
        let face = prototypes.face(&number, 0)?;
        let is_token = face["card_type"]
            .as_str()
            .is_some_and(|kind| kind.contains("トークン"));
        if token.is_some() && (!is_token || program.face_ordinals.len() != 1) {
            return Err(invalid("token identity requires one token face"));
        }
        if is_token {
            let name = token
                .and_then(|token| token["name"].as_str())
                .or_else(|| face["name"].as_str())
                .ok_or_else(|| invalid("token requires a rules name"))?
                .to_owned();
            prototypes.token_definitions.insert(
                number.clone(),
                super::TokenDefinition {
                    name,
                    template: token.is_some_and(|token| token["template"] == true),
                    complete: true,
                },
            );
        }
        if !prototypes.cards.contains_key(&number)
            || prototypes.programs.contains_key(&number)
            || prototypes.rejected.contains_key(&number)
            || !identities.insert(program.card_id.clone())
            || current.insert(number, Arc::new(program)).is_some()
        {
            return Err(invalid("current selection identity or prototype overlap"));
        }
    }
    prototypes.validate_token_templates()?;
    if let Some((document, identity)) = rules {
        prototypes.bind_rules(snapshot, keywords, &selection.documents, document, identity)?;
    }
    let prototypes = Arc::new(prototypes);
    Ok(SelectedEffects {
        accepted_prototypes: Arc::clone(&prototypes),
        prototypes,
        accepted: current.clone(),
        current,
    })
}
