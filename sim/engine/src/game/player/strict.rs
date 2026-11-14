//! Whole-card admission and dependency closure before any player opening is published.

use super::{DeckConstruction, PlayerDeck, Viewer};
use crate::catalog::SelectedEffects;
use crate::game::Game;
use crate::ir::walk::{NodeRef, Traversal, Visitor};
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
    /// The selected source and snapshot faces cannot be matched.
    SourceMismatch,
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
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Even a rejected opponent deck is private before the opening.
        formatter
            .debug_struct("StrictAdmission")
            .finish_non_exhaustive()
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
            NodeRef::Action(Action::Equip { template, .. })
            | NodeRef::Action(Action::Transform {
                into: TokenAllocation::Single { template },
                ..
            }) => {
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
            _ => {}
        }
        Traversal::Descend
    }
}

fn closure(
    selected: &SelectedEffects,
    deck: &PlayerDeck,
) -> Result<(BTreeSet<String>, BTreeSet<AdmissionGap>)> {
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
    let mut visited = BTreeSet::new();
    let mut gaps = BTreeSet::new();
    while let Some(number) = pending.pop() {
        if !visited.insert(number.clone()) {
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
            gaps.insert(AdmissionGap {
                card_no: number,
                reason,
            });
            continue;
        }
        let Some(candidate) = selected.current.get(&number) else {
            continue;
        };
        let mut dependencies = Dependencies::default();
        for ability in &candidate.program.card().abilities {
            crate::ir::walk::ability(ability, &mut dependencies);
        }
        for name in dependencies.templates {
            match catalog.token_print(&name) {
                Ok(printing) => pending.push(printing),
                Err(_private) => {
                    gaps.insert(AdmissionGap {
                        card_no: number.clone(),
                        reason: AdmissionGapReason::MissingTokenTemplate,
                    });
                }
            }
        }
        if dependencies.dynamic {
            // A runtime name can come from either player's cards; every selectable template is required.
            for (printing, token) in &catalog.token_definitions {
                if token.alias_of.is_some() {
                    continue;
                }
                match catalog.token_print(&token.name) {
                    Ok(template) => pending.push(template),
                    Err(_private) => {
                        gaps.insert(AdmissionGap {
                            card_no: printing.clone(),
                            reason: AdmissionGapReason::MissingTokenTemplate,
                        });
                    }
                }
            }
        }
        let authored = &candidate.expanded["cards"][&candidate.card_no];
        pending.extend(
            authored["token_sources"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|link| link["card_no"].as_str().map(str::to_owned)),
        );
    }
    Ok((visited, gaps))
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
    /// Rejects damaged selection provenance or invalid rule bindings as authority load failures.
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
            .flat_map(|(cards, _)| cards.iter().cloned())
            .collect();
        let mut probe = Self::unopened(
            Arc::clone(&selected.prototypes),
            &Value::Null,
            &Value::Null,
            &Value::Null,
            "strict-admission",
        )?;
        let failures: BTreeMap<_, _> = probe.prepare_strict_cards(selected, &required);
        let mut report = StrictAdmission::default();
        for ((cards, mut gaps), result) in closures.into_iter().zip(&mut report.gaps) {
            gaps.extend(
                failures
                    .iter()
                    .filter(|(number, _)| cards.contains(*number))
                    .map(|(number, reason)| AdmissionGap {
                        card_no: number.clone(),
                        reason: *reason,
                    }),
            );
            result.extend(gaps);
        }
        Ok((probe, report))
    }
}
