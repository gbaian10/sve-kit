//! Whole-card admission and dependency closure before any player opening is published.

use super::{DeckConstruction, PlayerDeck, Viewer};
use crate::catalog::{Catalog, SelectedEffects};
use crate::game::Game;
use crate::ir::walk::{self, NodeRef, Traversal, Visitor};
use crate::ir::{Action, TokenAllocation, TokenTemplate};
use crate::{EngineFailure, Result};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use core::fmt;
use serde_json::Value;

/// A closed, player-safe explanation; authority diagnostics and card wording stay private.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdmissionGapReason {
    /// The pinned snapshot has no such printing.
    MissingSnapshot,
    /// No complete current-source program was selected, including prototype-only cards.
    MissingProgram,
    /// The author reader reported an unresolved source ruling.
    UnresolvedRuling,
    /// Whole-card compilation rejected a required runtime feature or shape.
    UnsupportedRuntime,
    /// A referenced token name has no complete, unambiguous template.
    MissingTokenTemplate,
}

/// One missing requirement of this seat's deck or its transitive dependencies.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AdmissionGap {
    /// Exact snapshot card number; for missing templates, the referring card number.
    pub card_no: String,
    /// What prevents strict admission.
    pub reason: AdmissionGapReason,
}

/// Authority-held result. Hosts must bind the viewer to the receiving player channel.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct StrictAdmission {
    gaps: [Vec<AdmissionGap>; 2],
}

impl StrictAdmission {
    /// Only this authenticated seat's missing cards and requirements.
    #[must_use]
    pub const fn gaps(&self, viewer: Viewer) -> &[AdmissionGap] {
        match viewer {
            Viewer::P1 => self.gaps[0].as_slice(),
            Viewer::P2 => self.gaps[1].as_slice(),
        }
    }

    /// Whether both decks and their complete dependencies can enter strict play.
    #[must_use]
    pub fn is_admitted(&self) -> bool {
        self.gaps.iter().all(Vec::is_empty)
    }
}

impl fmt::Debug for StrictAdmission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Even a rejected opponent deck is private before the opening.
        f.debug_struct("StrictAdmission").finish_non_exhaustive()
    }
}

/// Strict startup separates owner-scoped admission gaps from authority load failures.
#[derive(Debug, thiserror::Error)]
pub enum StrictStartError {
    /// Deliver only `gaps(viewer)` through each authenticated player channel.
    #[error("strict deck admission failed")]
    Admission(StrictAdmission),
    /// Authority-only diagnostics, never player messages.
    #[error("strict game initialization failed")]
    Initialization(#[from] EngineFailure),
}

#[derive(Default)]
struct Dependencies {
    templates: BTreeSet<String>,
    dynamic: bool,
}

#[derive(Default)]
struct DeckClosure {
    cards: BTreeSet<String>,
    gaps: BTreeSet<AdmissionGap>,
    dependencies: BTreeMap<String, BTreeSet<String>>,
}

impl DeckClosure {
    fn require_template(
        &mut self,
        catalog: &Catalog,
        name: &str,
        source: &str,
        required: &mut BTreeSet<String>,
    ) -> Result<()> {
        match catalog.token_print(name) {
            Ok(printing) => {
                required.insert(printing);
            }
            Err(EngineFailure::Unsupported(_private)) => {
                self.gaps.insert(AdmissionGap {
                    card_no: source.into(),
                    reason: AdmissionGapReason::MissingTokenTemplate,
                });
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }

    fn explains_runtime_rejection(&self, number: &str) -> bool {
        let mut pending = vec![number];
        let mut visited = BTreeSet::new();
        while let Some(card) = pending.pop() {
            if !visited.insert(card) {
                continue;
            }
            if self.gaps.iter().any(|gap| gap.card_no == card) {
                return true;
            }
            if let Some(dependencies) = self.dependencies.get(card) {
                pending.extend(dependencies.iter().map(String::as_str));
            }
        }
        false
    }
}

impl Visitor for Dependencies {
    fn enter(&mut self, node: NodeRef<'_>, _path: &str) -> Traversal {
        match node {
            NodeRef::Action(Action::Token { tokens, .. }) => {
                for token in tokens {
                    match &token.template {
                        TokenTemplate::Fixed { template } => {
                            self.templates.insert(template.0.clone());
                        }
                        TokenTemplate::Name { .. } => self.dynamic = true,
                    }
                }
            }
            NodeRef::Action(
                Action::Equip { template, .. }
                | Action::Transform {
                    into: TokenAllocation::Single { template },
                    ..
                },
            ) => {
                self.templates.insert(template.0.clone());
            }
            NodeRef::Action(Action::Transform {
                into:
                    TokenAllocation::Unrestricted { templates, .. }
                    | TokenAllocation::EachExactlyOnce { templates, .. },
                ..
            }) => {
                self.templates
                    .extend(templates.iter().map(|template| template.0.clone()));
            }
            NodeRef::Source(_)
            | NodeRef::Program(_)
            | NodeRef::Ability(_)
            | NodeRef::Control(_)
            | NodeRef::Instruction(_)
            | NodeRef::Action(_)
            | NodeRef::Effect(_)
            | NodeRef::CostSpec(_)
            | NodeRef::ObjectOperand(_) => {}
        }
        Traversal::Descend
    }
}

fn closure(selected: &SelectedEffects, deck: &PlayerDeck) -> Result<DeckClosure> {
    let catalog = &selected.prototypes;
    let mut pending = deck
        .main
        .iter()
        .chain(&deck.evolve)
        .cloned()
        .collect::<Vec<_>>();
    if let DeckConstruction::Title(title) = &deck.construction
        && catalog
            .rule_bindings
            .capability(title, "opening_ex_resource")
    {
        pending.push(catalog.rule_bindings.template("lesson_item")?.card_no);
    }
    let mut closure = DeckClosure::default();
    while let Some(number) = pending.pop() {
        if !closure.cards.insert(number.clone()) {
            continue;
        }
        let reason = if !catalog.cards.contains_key(&number) {
            Some(AdmissionGapReason::MissingSnapshot)
        } else if catalog.rejected.get(&number).is_some_and(|reasons| {
            reasons
                .iter()
                .any(|reason| reason == "source_unresolved_issue")
        }) {
            Some(AdmissionGapReason::UnresolvedRuling)
        } else if !selected.current.contains_key(&number) {
            Some(AdmissionGapReason::MissingProgram)
        } else {
            None
        };
        if let Some(reason) = reason {
            closure.gaps.insert(AdmissionGap {
                card_no: number,
                reason,
            });
            continue;
        }
        let Some(candidate) = selected.current.get(&number) else {
            continue;
        };
        let mut dependencies = Dependencies::default();
        let mut required = BTreeSet::new();
        for ability in &candidate.program.card().abilities {
            walk::ability(ability, &mut dependencies);
        }
        for name in dependencies.templates {
            closure.require_template(catalog, &name, &number, &mut required)?;
        }
        if dependencies.dynamic {
            // A runtime name can come from either player's cards; every selectable template is required.
            for (printing, token) in &catalog.token_definitions {
                if token.alias_of.is_some() {
                    continue;
                }
                closure.require_template(catalog, &token.name, printing, &mut required)?;
            }
        }
        let authored = &candidate.expanded["cards"][&candidate.card_no];
        required.extend(
            authored["token_sources"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|link| link["card_no"].as_str().map(str::to_owned)),
        );
        pending.extend(required.iter().cloned());
        closure.dependencies.insert(number, required);
    }
    Ok(closure)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Strict startup shares the game authority."
)]
impl Game {
    /// Check both decks, their evolve cards, generated EX resources and token dependencies.
    /// No game, opening candidate or opponent card list is published.
    ///
    /// # Errors
    /// Rejects damaged selection provenance, source mismatches or authority admission failures.
    pub fn check_player_decks(
        selected: &SelectedEffects,
        decks: &[PlayerDeck; 2],
    ) -> Result<StrictAdmission> {
        Self::prepare_strict_player_decks(selected, decks).map(|(_, report)| report)
    }

    pub(super) fn prepare_strict_player_decks(
        selected: &SelectedEffects,
        decks: &[PlayerDeck; 2],
    ) -> Result<(Self, StrictAdmission)> {
        selected.check_runtime()?;
        let closures = [closure(selected, &decks[0])?, closure(selected, &decks[1])?];
        let required = closures
            .iter()
            .flat_map(|closure| closure.cards.iter().cloned())
            .collect();
        let mut probe = Self::unopened(
            Arc::clone(&selected.prototypes),
            &Value::Null,
            &Value::Null,
            &Value::Null,
            "strict-admission",
        )?;
        let failures = probe.prepare_strict_cards(selected, &required)?;
        let mut report = StrictAdmission::default();
        for (closure, result) in closures.into_iter().zip(&mut report.gaps) {
            let mut gaps = closure.gaps.clone();
            for (number, reason) in &failures {
                if closure.cards.contains(number) && !closure.explains_runtime_rejection(number) {
                    gaps.insert(AdmissionGap {
                        card_no: number.clone(),
                        reason: *reason,
                    });
                }
            }
            result.extend(gaps);
        }
        Ok((probe, report))
    }
}
