//! Internal batch selection; current programs remain candidates pending runtime admission.

use super::current::CurrentCard;
use super::{Card, Catalog, EngineFailure, EngineIdentityInput, Result, invalid};
use crate::game::internal::{AdmittedCard, CandidateAdmission};
use crate::ir::lowering::{CurrentCardProgram, ExpandedLowerer};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use core::{fmt, ptr::from_ref};
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Mutex, OnceLock};

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
    accepted: Arc<BTreeMap<String, Arc<CandidateAdmission>>>,
    accepted_prototypes: Arc<Catalog>,
    runtime: Arc<OnceLock<Result<Arc<RuntimeCards>>>>,
}

#[derive(Clone, Default)]
pub(crate) struct RuntimeCards {
    pub(crate) cards: BTreeMap<String, Arc<AdmittedCard>>,
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
                    .is_none_or(|accepted| !Arc::ptr_eq(candidate, accepted.candidate()))
            })
        {
            return Err(invalid("current selection changed after acceptance"));
        }
        Ok(())
    }

    /// The private accepted copies; use them only after [`Self::check_runtime`] passed.
    pub(crate) fn accepted(&self) -> &BTreeMap<String, Arc<CandidateAdmission>> {
        &self.accepted
    }
}

/// Reusable loader for related current selections.
///
/// Exact snapshot records and author inputs share immutable facts and candidates.
/// A shared candidate also shares the admission work that depends on its program alone.
/// Snapshot closure, prototype, token and rule checks still run for every selection.
/// Drop the loader when the related selections are no longer needed to release
/// its cached facts and candidates.
/// Selection preparation runs concurrently; only cache access holds a lock.
#[derive(Debug)]
pub struct CurrentSelectionLoader {
    lowerer: ExpandedLowerer,
    candidates: Mutex<BTreeMap<String, Arc<CandidateAdmission>>>,
    snapshot_cards: Mutex<BTreeMap<String, Arc<Card>>>,
}

impl CurrentSelectionLoader {
    /// Prepare a reusable compiler using the bundled expanded schema.
    ///
    /// # Errors
    /// Rejects an invalid bundled schema.
    pub fn new() -> Result<Self> {
        Ok(Self {
            lowerer: ExpandedLowerer::new().map_err(invalid)?,
            candidates: Mutex::default(),
            snapshot_cards: Mutex::default(),
        })
    }

    /// Select complete current candidates, sharing only identical author inputs.
    ///
    /// # Errors
    /// Rejects the same invalid selections as [`Catalog::from_current_selection`].
    pub fn select(&self, snapshot: &str, keywords: &str, input: &Value) -> Result<SelectedEffects> {
        select_using(
            snapshot,
            keywords,
            input,
            None,
            |candidate| self.compile(candidate),
            |line| self.read_card(line),
        )
    }

    /// Select current candidates and bind the exact snapshot's rules.
    ///
    /// # Errors
    /// Also rejects invalid rules and inconsistent rule identities.
    pub fn select_with_rules(
        &self,
        snapshot: &str,
        keywords: &str,
        input: &Value,
        rules: &str,
        identity: &EngineIdentityInput,
    ) -> Result<SelectedEffects> {
        select_using(
            snapshot,
            keywords,
            input,
            Some((rules, identity)),
            |candidate| self.compile(candidate),
            |line| self.read_card(line),
        )
    }

    /// Check the whole selection and register every printing without lowering any author.
    ///
    /// # Errors
    /// Rejects the malformed handoffs, overlaps and rule identities a full selection rejects;
    /// an author's own input is only checked once one of its printings is requested.
    pub(crate) fn begin<'input>(
        &'input self,
        snapshot: &str,
        keywords: &str,
        input: &'input Value,
        rules: Option<(&str, &EngineIdentityInput)>,
    ) -> Result<PendingSelection<'input>> {
        let handoff = read_handoff(input)?;
        let mut prototypes =
            Catalog::read_documents_using(snapshot, keywords, &handoff.documents, |line| {
                self.read_card(line)
            })?;
        let mut owners = BTreeMap::new();
        let mut identities = BTreeSet::new();
        for author in handoff.authors {
            let (candidate, printings) = match author {
                Author::Accepted { input, printings } => (input, printings),
                Author::Rejected { card_no, reason } => {
                    record_rejection(&mut prototypes, card_no, reason)?;
                    continue;
                }
            };
            let owner =
                Owner::of_input(candidate).ok_or_else(|| invalid("invalid current selection"))?;
            if !identities.insert(owner.card_id)
                || !printings.iter().any(|number| number == owner.card_no)
            {
                return Err(invalid(
                    "current selection identity or representative missing",
                ));
            }
            for number in printings {
                register_printing(&mut prototypes, &number, &owner)?;
                if owners.insert(number, candidate).is_some() {
                    return Err(invalid("duplicate current printing"));
                }
            }
        }
        prototypes.validate_token_templates()?;
        if let Some((document, identity)) = rules {
            prototypes.bind_rules(snapshot, keywords, &handoff.documents, document, identity)?;
        }
        Ok(PendingSelection {
            loader: self,
            prototypes,
            owners,
            loaded: BTreeMap::new(),
        })
    }

    fn read_card(&self, line: &str) -> Result<Arc<Card>> {
        // A printing number alone cannot identify changed facts in related snapshots.
        let cached = self
            .snapshot_cards
            .lock()
            .map_err(invalid)?
            .get(line)
            .cloned();
        if let Some(card) = cached {
            return Ok(card);
        }
        let card = Catalog::read_snapshot_card(line)?;
        let key = line.to_owned();
        let mut cards = self.snapshot_cards.lock().map_err(invalid)?;
        Ok(Arc::clone(cards.entry(key).or_insert(card)))
    }

    fn compile(&self, input: &Value) -> Result<Arc<CandidateAdmission>> {
        // Key the complete input, including source evidence, rather than trusting caller hashes.
        let key = serde_json::to_string(input).map_err(invalid)?;
        let cached = self.candidates.lock().map_err(invalid)?.get(&key).cloned();
        if let Some(candidate) = cached {
            return Ok(candidate);
        }
        // Lower outside the lock so unrelated selections can prepare in parallel.
        let candidate = accept(self.lowerer.lower_current(input).map_err(invalid)?);
        let mut candidates = self.candidates.lock().map_err(invalid)?;
        Ok(Arc::clone(candidates.entry(key).or_insert(candidate)))
    }
}

pub(super) fn select(
    snapshot: &str,
    keywords: &str,
    handoff: &Value,
    rules: Option<(&str, &EngineIdentityInput)>,
) -> Result<SelectedEffects> {
    let lowerer = ExpandedLowerer::new().map_err(invalid)?;
    select_using(
        snapshot,
        keywords,
        handoff,
        rules,
        |input| lowerer.lower_current(input).map(accept).map_err(invalid),
        Catalog::read_snapshot_card,
    )
}

fn accept(candidate: CurrentCardProgram) -> Arc<CandidateAdmission> {
    Arc::new(CandidateAdmission::new(Arc::new(candidate)))
}

/// One author of the handoff, borrowed so an input that is never lowered is never copied.
enum Author<'handoff> {
    Accepted {
        input: &'handoff Value,
        printings: Vec<String>,
    },
    Rejected {
        card_no: &'handoff str,
        reason: &'handoff str,
    },
}

struct Handoff<'handoff> {
    documents: Vec<(String, String)>,
    authors: Vec<Author<'handoff>>,
}

/// Exactly `documents` and `authors`; every author is exactly one of the two closed shapes.
fn read_handoff(handoff: &Value) -> Result<Handoff<'_>> {
    let read = || {
        let fields = handoff.as_object().filter(|fields| fields.len() == 2)?;
        let documents = Vec::deserialize(fields.get("documents")?).ok()?;
        let authors = fields
            .get("authors")?
            .as_array()?
            .iter()
            .map(read_author)
            .collect::<Option<_>>()?;
        Some(Handoff { documents, authors })
    };
    read().ok_or_else(|| invalid("invalid current selection"))
}

fn read_author(author: &Value) -> Option<Author<'_>> {
    let fields = author.as_object().filter(|fields| fields.len() == 3)?;
    match fields.get("status")?.as_str()? {
        "accepted" => Some(Author::Accepted {
            input: fields.get("input")?,
            printings: Vec::deserialize(fields.get("printings")?).ok()?,
        }),
        "rejected" => Some(Author::Rejected {
            card_no: fields.get("card_no")?.as_str()?,
            reason: fields.get("reason")?.as_str()?,
        }),
        _ => None,
    }
}

/// What registering a printing reads from its owner, before or after lowering.
#[derive(PartialEq, Eq)]
struct Owner<'owner> {
    card_id: &'owner str,
    card_no: &'owner str,
    token: Option<&'owner Value>,
    faces: usize,
}

impl<'owner> Owner<'owner> {
    fn of_program(program: &'owner CurrentCardProgram) -> Self {
        Self {
            card_id: &program.card_id,
            card_no: &program.card_no,
            token: program
                .expanded
                .pointer(&format!("/cards/{}/token", program.card_no)),
            faces: program.face_ordinals.len(),
        }
    }

    /// The same facts from an author input that has not been lowered.
    fn of_input(input: &'owner Value) -> Option<Self> {
        let card_no = input.get("card_no")?.as_str()?;
        let direct = input.get("admission").and_then(Value::as_str) == Some("direct");
        Some(Self {
            card_id: input.get("card_id")?.as_str()?,
            card_no,
            token: input
                .get("tree")?
                .pointer(&format!("/cards/{card_no}/token")),
            // Only direct admission records face ordinals; other lowering leaves them empty.
            faces: if direct {
                input.pointer("/source_payload/faces")?.as_array()?.len()
            } else {
                0
            },
        })
    }
}

fn record_rejection(prototypes: &mut Catalog, card_no: &str, reason: &str) -> Result<()> {
    if reason != "source_unresolved_issue" {
        return Err(EngineFailure::Unsupported(format!(
            "current card rejected: {card_no}: {reason}"
        )));
    }
    if !prototypes.cards.contains_key(card_no)
        || prototypes.programs.contains_key(card_no)
        || prototypes
            .rejected
            .insert(card_no.into(), vec![reason.into()])
            .is_some()
    {
        return Err(invalid("unresolved current card overlaps a prototype"));
    }
    Ok(())
}

fn check_token_sources(prototypes: &Catalog, program: &CurrentCardProgram) -> Result<()> {
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
    Ok(())
}

/// Consume `CurrentAuthorSources.selection_input()` from the trusted Python reader.
///
/// The reader owns index closure and permanent-identity overlap checks. This
/// handoff is internal and deliberately not a serialized catalog/save format.
///
/// # Errors
/// Rejects the entire selection on any rejected card, overlap or D0 failure;
/// callers never receive a partial selection or an implicit prototype fallback.
fn select_using(
    snapshot: &str,
    keywords: &str,
    handoff: &Value,
    rules: Option<(&str, &EngineIdentityInput)>,
    mut compile: impl FnMut(&Value) -> Result<Arc<CandidateAdmission>>,
    read_card: impl FnMut(&str) -> Result<Arc<Card>>,
) -> Result<SelectedEffects> {
    let handoff = read_handoff(handoff)?;
    let mut prototypes =
        Catalog::read_documents_using(snapshot, keywords, &handoff.documents, read_card)?;
    let mut current = BTreeMap::new();
    let mut accepted = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for author in handoff.authors {
        let (input, printings) = match author {
            Author::Accepted { input, printings } => (input, printings),
            Author::Rejected { card_no, reason } => {
                record_rejection(&mut prototypes, card_no, reason)?;
                continue;
            }
        };
        let admission = compile(input)?;
        let program = admission.candidate();
        if !identities.insert(program.card_id.clone()) || !printings.contains(&program.card_no) {
            return Err(invalid(
                "current selection identity or representative missing",
            ));
        }
        let owner = Owner::of_program(program);
        for number in printings {
            register_printing(&mut prototypes, &number, &owner)?;
            if current
                .insert(number.clone(), Arc::clone(program))
                .is_some()
            {
                return Err(invalid("duplicate current printing"));
            }
            accepted.insert(number, Arc::clone(&admission));
        }
    }
    prototypes.validate_token_templates()?;
    for program in current.values() {
        check_token_sources(&prototypes, program)?;
    }
    if let Some((document, identity)) = rules {
        prototypes.bind_rules(snapshot, keywords, &handoff.documents, document, identity)?;
    }
    let prototypes = Arc::new(prototypes);
    Ok(SelectedEffects {
        runtime: Arc::default(),
        accepted_prototypes: Arc::clone(&prototypes),
        prototypes,
        accepted: Arc::new(accepted),
        current,
    })
}

/// A checked selection whose authors are lowered only when a printing is requested.
///
/// Every printing is already registered from its author's raw input, so the catalogue,
/// token names and rule bindings are complete; unrequested authors stay unverified.
pub(crate) struct PendingSelection<'input> {
    loader: &'input CurrentSelectionLoader,
    prototypes: Catalog,
    owners: BTreeMap<String, &'input Value>,
    loaded: BTreeMap<String, Arc<CandidateAdmission>>,
}

impl PendingSelection<'_> {
    /// The complete catalogue, and the candidate for one printing lowered on first request.
    pub(crate) fn parts(
        &mut self,
    ) -> (
        &Catalog,
        impl FnMut(&str) -> Result<Option<Arc<CurrentCardProgram>>>,
    ) {
        let Self {
            loader,
            prototypes,
            owners,
            loaded,
        } = self;
        let candidate = |number: &str| {
            if let Some(accepted) = loaded.get(number) {
                return Ok(Some(Arc::clone(accepted.candidate())));
            }
            let Some(input) = owners.get(number) else {
                return Ok(None);
            };
            let accepted = loader.compile(input)?;
            // Its printings were registered from the raw input; lowering must state the same facts.
            if Owner::of_input(input) != Some(Owner::of_program(accepted.candidate())) {
                return Err(invalid(
                    "current selection identity or representative missing",
                ));
            }
            let program = Arc::clone(accepted.candidate());
            loaded.insert(number.to_owned(), accepted);
            Ok(Some(program))
        };
        (prototypes, candidate)
    }

    /// Accept exactly the requested printings.
    pub(crate) fn finish(self) -> Result<SelectedEffects> {
        for accepted in self.loaded.values() {
            check_token_sources(&self.prototypes, accepted.candidate())?;
        }
        let current = self
            .loaded
            .iter()
            .map(|(number, accepted)| (number.clone(), Arc::clone(accepted.candidate())))
            .collect();
        let prototypes = Arc::new(self.prototypes);
        Ok(SelectedEffects {
            runtime: Arc::default(),
            accepted_prototypes: Arc::clone(&prototypes),
            prototypes,
            accepted: Arc::new(self.loaded),
            current,
        })
    }
}

fn register_printing(prototypes: &mut Catalog, number: &str, owner: &Owner<'_>) -> Result<()> {
    if prototypes.programs.contains_key(number) || prototypes.rejected.contains_key(number) {
        return Err(invalid("current selection identity or prototype overlap"));
    }
    let face = prototypes.face(number, 0)?;
    let is_token = face["card_type"]
        .as_str()
        .is_some_and(|kind| kind.contains("トークン"));
    if owner.token.is_some() && (!is_token || owner.faces != 1) {
        return Err(invalid("token identity requires one token face"));
    }
    if is_token {
        let name = owner
            .token
            .and_then(|token| token["name"].as_str())
            .or_else(|| prototypes.cards.get(owner.card_no)?.faces.first()?["name"].as_str())
            .ok_or_else(|| invalid("token requires a rules name"))?
            .to_owned();
        prototypes.token_definitions.insert(
            number.into(),
            super::TokenDefinition {
                name,
                name_override: owner.token.is_some(),
                template: owner.token.is_some_and(|token| token["template"] == true),
                alias_of: (number != owner.card_no).then(|| owner.card_no.to_owned()),
                complete: true,
            },
        );
    }
    Ok(())
}
