//! Internal batch selection; current programs remain candidates pending runtime admission.

use super::current::CurrentCard;
use super::{Catalog, EngineFailure, EngineIdentityInput, Result, invalid};
use crate::ir::WellFormedCardProgram;
use crate::ir::lowering::{CurrentCardProgram, ExpandedLowerer};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use core::{fmt, ptr::from_ref};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;

/// Disjoint prototype programs and complete, current-source checked D0 candidates.
#[derive(Clone, Debug)]
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
    runtime: Arc<OnceLock<Result<Arc<RuntimeCards>>>>,
}

#[derive(Clone, Default)]
pub(crate) struct RuntimeCards {
    pub(crate) cards: BTreeMap<String, Arc<WellFormedCardProgram>>,
    pub(crate) current: BTreeMap<String, Arc<CurrentCard>>,
}

impl fmt::Debug for RuntimeCards {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.current.is_empty() {
            return f
                .debug_struct("RuntimeCards")
                .field("cards", &self.cards)
                .field("current", &self.current)
                .finish();
        }
        // Immutable definitions share an address; expanding their trees dominates rollback diagnostics.
        f.debug_struct("RuntimeCards")
            .field("address", &from_ref(self))
            .field("cards", &self.cards.len())
            .field("current", &self.current.len())
            .finish_non_exhaustive()
    }
}

impl SelectedEffects {
    pub(crate) fn runtime_cards(
        &self,
        prepare: impl FnOnce() -> Result<Arc<RuntimeCards>>,
    ) -> Result<Arc<RuntimeCards>> {
        self.runtime.get_or_init(prepare).clone()
    }

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
    Accepted {
        input: Value,
        printings: Vec<String>,
    },
    Rejected {
        card_no: String,
        reason: String,
    },
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
        let (candidate, printings) = match author {
            Author::Accepted { input, printings } => (input, printings),
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
        if !identities.insert(program.card_id.clone()) || !printings.contains(&program.card_no) {
            return Err(invalid(
                "current selection identity or representative missing",
            ));
        }
        let program = Arc::new(program);
        for number in printings {
            register_printing(&mut prototypes, &number, &program)?;
            if current.insert(number, Arc::clone(&program)).is_some() {
                return Err(invalid("duplicate current printing"));
            }
        }
    }
    prototypes.validate_token_templates()?;
    for program in current.values() {
        let card = &program.expanded["cards"][&program.card_no];
        for link in card["token_sources"].as_array().into_iter().flatten() {
            let number = link["card_no"]
                .as_str()
                .ok_or_else(|| invalid("invalid token source"))?;
            if prototypes
                .token_definitions
                .get(number)
                .is_none_or(|token| !token.complete)
            {
                return Err(invalid("token source requires a complete token card"));
            }
        }
    }
    if let Some((document, identity)) = rules {
        prototypes.bind_rules(snapshot, keywords, &selection.documents, document, identity)?;
    }
    let prototypes = Arc::new(prototypes);
    Ok(SelectedEffects {
        runtime: Arc::default(),
        accepted_prototypes: Arc::clone(&prototypes),
        prototypes,
        accepted: current.clone(),
        current,
    })
}

fn register_printing(
    prototypes: &mut Catalog,
    number: &str,
    program: &CurrentCardProgram,
) -> Result<()> {
    if prototypes.programs.contains_key(number) || prototypes.rejected.contains_key(number) {
        return Err(invalid("current selection identity or prototype overlap"));
    }
    let token = program
        .expanded
        .pointer(&format!("/cards/{}/token", program.card_no));
    let face = prototypes.face(number, 0)?;
    let is_token = face["card_type"]
        .as_str()
        .is_some_and(|kind| kind.contains("トークン"));
    if token.is_some() && (!is_token || program.face_ordinals.len() != 1) {
        return Err(invalid("token identity requires one token face"));
    }
    if is_token {
        let name = token
            .and_then(|token| token["name"].as_str())
            .or_else(|| prototypes.cards.get(&program.card_no)?.faces.first()?["name"].as_str())
            .ok_or_else(|| invalid("token requires a rules name"))?
            .to_owned();
        prototypes.token_definitions.insert(
            number.into(),
            super::TokenDefinition {
                name,
                name_override: token.is_some(),
                template: token.is_some_and(|token| token["template"] == true),
                alias_of: (number != program.card_no).then(|| program.card_no.clone()),
                complete: true,
            },
        );
    }
    Ok(())
}
