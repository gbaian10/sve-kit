//! Whole-card admission and dependency closure before any player opening is published.

use super::{DeckConstruction, PlayerDeck, Viewer};
use crate::battle::BattleReferences;
use crate::catalog::{
    Catalog, CurrentSelectionLoader, EngineIdentityInput, SelectedEffects, SnapshotPrograms,
};
use crate::game::Game;
use crate::ir::lowering::CurrentCardProgram;
use crate::ir::walk::{self, NodeRef, Traversal, Visitor};
use crate::ir::{Action, PlayerRef, RideDriveOperation, TokenAllocation, TokenTemplate};
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
    /// The pinned snapshot does not verify the printed Trigger icons this card may need.
    MissingTriggerIcons,
    /// An active printed construction restriction rejects this card's copies.
    DeckRuleViolation,
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
    trigger_icons: TriggerIconUse,
}

#[derive(Default)]
struct TriggerIconUse {
    reads_icon: bool,
    starts_drive: bool,
    control_transfer: Option<ControlTransfer>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlTransfer {
    Receive,
    MayGive,
}

#[derive(Default)]
struct DeckClosure {
    cards: BTreeSet<String>,
    gaps: BTreeSet<AdmissionGap>,
    dependencies: BTreeMap<String, BTreeSet<String>>,
    trigger_icons: BTreeMap<String, TriggerIconUse>,
}

impl DeckClosure {
    fn can_give_control(&self) -> bool {
        self.trigger_icons
            .values()
            .any(|uses| uses.control_transfer == Some(ControlTransfer::MayGive))
    }

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
            NodeRef::Action(
                Action::DriveCheck { .. }
                | Action::RideDrive {
                    operation: RideDriveOperation::GainDrive {},
                    ..
                },
            ) => self.trigger_icons.starts_drive = true,
            NodeRef::Action(Action::Transfer {
                destination_controller,
                ..
            }) => {
                if matches!(destination_controller, PlayerRef::RequireOne { .. }) {
                    self.trigger_icons.control_transfer = Some(ControlTransfer::MayGive);
                } else {
                    self.trigger_icons
                        .control_transfer
                        .get_or_insert(ControlTransfer::Receive);
                }
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

    fn require(&mut self, code: &'static str, _path: &str) {
        self.trigger_icons.reads_icon |= code == "internal.BooleanState.HasTrigger";
    }
}

/// `provide` answers one printing at a time, so a lazy selection lowers only what is reached.
fn closure(
    catalog: &Catalog,
    provide: &mut impl FnMut(&str) -> Result<Option<Arc<CurrentCardProgram>>>,
    deck: &PlayerDeck,
) -> Result<DeckClosure> {
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
        let found = if !catalog.cards.contains_key(&number) {
            Err(AdmissionGapReason::MissingSnapshot)
        } else if catalog.rejected.get(&number).is_some_and(|reasons| {
            reasons
                .iter()
                .any(|reason| reason == "source_unresolved_issue")
        }) {
            Err(AdmissionGapReason::UnresolvedRuling)
        } else {
            provide(&number)?.ok_or(AdmissionGapReason::MissingProgram)
        };
        let candidate = match found {
            Ok(candidate) => candidate,
            Err(reason) => {
                closure.gaps.insert(AdmissionGap {
                    card_no: number,
                    reason,
                });
                continue;
            }
        };
        let mut dependencies = Dependencies::default();
        let mut required = BTreeSet::new();
        for ability in &candidate.program.card().abilities {
            walk::ability(ability, &mut dependencies);
        }
        closure
            .trigger_icons
            .insert(number.clone(), dependencies.trigger_icons);
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
        required.extend(candidate.token_printings.iter().cloned());
        pending.extend(required.iter().cloned());
        closure.dependencies.insert(number, required);
    }
    Ok(closure)
}

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

    /// Preflight the complete match before publishing any battle input.
    ///
    /// Uses the same main/evolve, generated-resource and transitive token closure
    /// as strict startup. Whole-card admission checks every branch and nested
    /// definition; all admitted player shapes have battle adapters. Source
    /// references are checked throughout that closure, including untaken branches.
    /// Hosts still authenticate `gaps(viewer)` and start the same selection/decks.
    ///
    /// # Errors
    /// Rejects invalid authority inputs or required references. These errors are
    /// host failures, never player diagnostics; an unadmitted deck has scoped gaps.
    pub fn check_battle_decks(
        selected: &SelectedEffects,
        decks: &[PlayerDeck; 2],
        references: &BattleReferences,
    ) -> Result<StrictAdmission> {
        references
            .validate_catalog(&selected.prototypes)
            .map_err(|_reference| crate::invalid("battle reference background mismatch"))?;
        let (admitted, report) = Self::prepare_strict_player_decks(selected, decks)?;
        if report.is_admitted() {
            admitted.validate_battle_sources(references)?;
        }
        Ok(report)
    }

    /// Select only what these decks can reach: main and evolve cards, generated EX resources
    /// and token dependencies. Other authors are registered but never lowered or checked.
    /// The result serves [`Self::check_player_decks`] and [`Self::start_player_game`] for
    /// these decks; the whole selection is proven loadable by the repository tests instead.
    ///
    /// # Errors
    /// Rejects a malformed selection, inconsistent rules, or a reachable author that fails
    /// lowering. A deck card without any author is reported later as an admission gap.
    pub fn select_player_decks(
        loader: &CurrentSelectionLoader,
        snapshot: &str,
        keywords: &str,
        input: &Value,
        rules: Option<(&str, &EngineIdentityInput)>,
        decks: &[PlayerDeck; 2],
    ) -> Result<SelectedEffects> {
        let mut pending = loader.begin(snapshot, keywords, input, rules)?;
        {
            let (catalog, mut provide) = pending.parts();
            for deck in decks {
                closure(catalog, &mut provide, deck)?;
            }
        }
        pending.finish()
    }

    /// Select the executable closure for these decks from the prepared public snapshot.
    ///
    /// # Errors
    /// Refuses a reachable program that cannot lower or has unresolved dependencies.
    pub fn select_snapshot_decks(
        programs: &SnapshotPrograms,
        decks: &[PlayerDeck; 2],
    ) -> Result<SelectedEffects> {
        programs.select(|catalogue, lower| {
            for deck in decks {
                closure(catalogue, &mut |number| lower(number), deck)?;
            }
            Ok(())
        })
    }

    pub(super) fn prepare_strict_player_decks(
        selected: &SelectedEffects,
        decks: &[PlayerDeck; 2],
    ) -> Result<(Self, StrictAdmission)> {
        selected.check_runtime()?;
        let mut provide = |number: &str| Ok(selected.current.get(number).cloned());
        let closures = [
            closure(&selected.prototypes, &mut provide, &decks[0])?,
            closure(&selected.prototypes, &mut provide, &decks[1])?,
        ];
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
        let [first_title, second_title] = decks
            .each_ref()
            .map(|deck| matches!(deck.construction, DeckConstruction::Title(_)));
        // CR 14.4.5 uses the checking player's basis, including after a control transfer.
        let drive_icon_requirements = [
            first_title || (second_title && closures[0].can_give_control()),
            second_title || (first_title && closures[1].can_give_control()),
        ];
        for ((closure, result), (deck, drive_needs_icons)) in closures
            .into_iter()
            .zip(&mut report.gaps)
            .zip(decks.iter().zip(drive_icon_requirements))
        {
            let mut gaps = closure.gaps.clone();
            let is_title = matches!(deck.construction, DeckConstruction::Title(_));
            gaps.extend(
                closure
                    .trigger_icons
                    .iter()
                    .filter(|(_, uses)| {
                        uses.reads_icon
                            || (drive_needs_icons && uses.starts_drive)
                            || (is_title && uses.control_transfer.is_some())
                    })
                    .map(|(card_no, _)| AdmissionGap {
                        card_no: card_no.clone(),
                        reason: AdmissionGapReason::MissingTriggerIcons,
                    }),
            );
            gaps.extend(
                super::check_card_deck_rules(selected, deck)?
                    .into_iter()
                    .map(|card_no| AdmissionGap {
                        card_no,
                        reason: AdmissionGapReason::DeckRuleViolation,
                    }),
            );
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
