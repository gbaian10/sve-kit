//! Search retains its private candidate policy over the shared property-only condition core.

use super::card::selection::constraints::{
    DistinctProperties, PropertyKey, check_count, within_original_cost_cap,
};
use super::{
    Execution, Fact, Game, InputDomain, InstructionOutcome, OrderCard, Result, Wait, contains,
    invalid, seat_name, unsupported,
};
use crate::ir::observation_selection::{CardTotals, card_totals};
use crate::ir::printed_keywords;
use crate::ir::{
    AbilityOrigin, Action, Binding, Bool, Cardinality, CharacteristicView, DecisionId, Expr,
    Instruction, KeywordId, MoveDestination, MoveMethod, MoveResult, MoveSpec, Nat, ObjectOperand,
    ObjectProperty, ObjectRef, ObjectView, OwnerZone, PlayerId, PlayerRelation, Posture, Predicate,
    Requirement, Requirements, RuntimeObjectView, RuntimeZoneRef, SearchDestination, SearchOutcome,
    SearchPartition, SearchPartitionAnswer, SearchSelectionAnswer, SelectionView, Selector,
    SnapshotId, ZoneKind, ZoneRef,
};
use alloc::collections::{BTreeSet, VecDeque};
use serde_json::{Value, json};

mod keywords;
mod replacement;
pub(super) use replacement::ReplacementWait;

/// Frozen selection constraints visible only to the authenticated searcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchDomain {
    /// Printed faces with handles scoped to this question's token.
    pub cards: Vec<OrderCard>,
    /// Effective total bounds, including the conditional-search exception.
    pub cardinality: Cardinality,
    /// Printed groups, all answered together; empty groups remain present.
    pub groups: Option<Vec<SearchGroup>>,
}

/// One group's intersection with the global filter, without physical identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchGroup {
    /// Handles from the same `SearchDomain`.
    pub handles: Vec<u64>,
    /// Effective bounds for this group.
    pub cardinality: Cardinality,
}

/// Wire-facing selection shape; the authority converts handles to typed old generations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchAnswer {
    /// No grouping constraint.
    Ungrouped(Vec<u64>),
    /// Every printed group, including empty entries.
    Grouped(Vec<Vec<u64>>),
}

/// Frozen candidates for a searched destination or a movement capacity allocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchPartitionDomain {
    /// Fresh handles scoped to this allocation question.
    pub cards: Vec<OrderCard>,
    /// Printed bounds truncated to the number of remaining found members.
    pub cardinality: Cardinality,
}

#[derive(Clone, Debug)]
struct PartitionContinuation {
    instruction: Box<Instruction>,
    player: PlayerId,
    outcome: SearchOutcome,
    remaining: Vec<ObjectRef>,
    destinations: VecDeque<SearchPartition>,
    next_handle: u64,
    capacity_remainder: Option<MoveDestination>,
    entry_posture: Option<Posture>,
}

#[derive(Clone, Debug)]
pub(super) struct PartitionWait {
    continuation: PartitionContinuation,
    partition: SearchPartition,
    domain: SearchPartitionDomain,
    deck: Vec<String>,
    snapshot: SnapshotId,
}

#[derive(Clone, Debug)]
pub(super) struct SearchWait {
    instruction: Box<Instruction>,
    player: PlayerId,
    deck: Vec<String>,
    members: Vec<ObjectRef>,
    group_members: Option<Vec<Vec<ObjectRef>>>,
    domain: SearchDomain,
    snapshot: SnapshotId,
    conditional: bool,
    distinct_names: bool,
    names: Vec<Vec<String>>,
    /// 元のコストの合計がN以下 over every found card.
    cost_cap: Option<i64>,
}

pub(super) fn packet(domain: &SearchDomain) -> Value {
    json!({"kind":"search","cards":domain.cards.iter().map(|card| json!({"handle":card.handle,"card":card.card,"name":card.name})).collect::<Vec<_>>(),
        "minimum":domain.cardinality.minimum.0,"maximum":domain.cardinality.maximum.map(|n| n.0),
        "groups":domain.groups.as_ref().map(|groups| groups.iter().map(|group| json!({"handles":group.handles,"minimum":group.cardinality.minimum.0,"maximum":group.cardinality.maximum.map(|n| n.0)})).collect::<Vec<_>>())})
}

pub(super) fn check_predicate(value: &Expr<Bool>, candidate: Binding<ObjectRef>) -> Result<bool> {
    super::conditions::ConditionPlan::compile(value, candidate, super::conditions::Policy::Search)
        .map(|plan| plan.conditional())
}

fn candidate_view(view: &ObjectView, candidate: Binding<ObjectRef>) -> bool {
    matches!(view, ObjectView::Current { object } if matches!(object.as_ref(), ObjectOperand::Bound { binding } if *binding == candidate))
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    let Action::Search {
        player,
        candidate,
        filter,
        groups,
        destination,
        cardinality,
        continuation,
        ..
    } = action
    else {
        return Ok(());
    };
    let conditional = check_predicate(filter, *candidate)?
        || groups
            .as_ref()
            .is_some_and(|g| g.distinct_by == Some(ObjectProperty::Names));
    if let Some(groups) = groups {
        if matches!(
            card_totals(&groups.aggregate_bounds),
            CardTotals::Unsupported
        ) || !matches!(groups.distinct_by, None | Some(ObjectProperty::Names))
        {
            return Err(unsupported("search aggregates or distinct property"));
        }
        for group in &groups.groups {
            let Selector::Zone {
                players,
                zone: ZoneKind::Deck,
                view: SelectionView::Current {},
                binding,
                predicate,
            } = &group.selector
            else {
                return Err(unsupported("search group selector"));
            };
            if players.relative_to != *player
                || players.relation != PlayerRelation::SelfPlayer
                || !contains(players.cardinality, Nat(1))
                || (!check_predicate(predicate, *binding)? && !conditional)
            {
                return Err(unsupported("search group classification or owner"));
            }
        }
    }
    let movement = match destination {
        Some(SearchDestination::Single { movement }) => movement,
        Some(SearchDestination::OptionalSelfReplacement {
            predicate,
            movement,
            ..
        }) if cardinality.maximum.is_some_and(|maximum| maximum.0 <= 1)
            && matches!(
                &movement.to,
                MoveDestination::Shared {
                    zone: ZoneRef {
                        kind: ZoneKind::Field,
                        ..
                    }
                }
            ) =>
        {
            replacement::check_predicate(predicate, continuation.found)?;
            movement
        }
        Some(SearchDestination::Partition { destinations })
            if matches!(destinations.as_slice(), [hand, cemetery]
                if hand.zone == (MoveDestination::Owners { zone: OwnerZone::Hand })
                && cemetery.zone == (MoveDestination::Owners { zone: OwnerZone::Cemetery })
                && cardinality.maximum.is_some_and(|maximum|
                    maximum.0.checked_sub(hand.cardinality.minimum.0)
                        .is_none_or(|needed| cemetery.cardinality.minimum.0 >= needed))) =>
        {
            // Unallocated found members have no admitted disposition; prove all choices consume the set.
            return Ok(());
        }
        _ => return Err(unsupported("search destination or incomplete allocation")),
    };
    check_movement(movement)?;
    if movement.initial_posture.is_some()
        && (!matches!(destination, Some(SearchDestination::Single { .. }))
            || groups.is_some()
            || cardinality.maximum != Some(Nat(1)))
    {
        return Err(unsupported("single ungrouped acted search"));
    }
    // Whether a found spell stays in the deck is unadmitted; a count-only search could be forced to pick one.
    if !conditional
        && groups.is_none()
        && matches!(
            &movement.to,
            MoveDestination::Shared {
                zone: ZoneRef {
                    kind: ZoneKind::Field,
                    ..
                }
            }
        )
    {
        return Err(unsupported("count-only search onto the field"));
    }
    Ok(())
}

fn check_movement(movement: &MoveSpec) -> Result<()> {
    let posture_allowed = movement.initial_posture.is_none()
        || matches!(
            (movement.initial_posture, &movement.to),
            (
                Some(Posture::Acted),
                MoveDestination::Shared {
                    zone: ZoneRef {
                        kind: ZoneKind::Field,
                        ..
                    }
                }
            )
        );
    if movement.position.is_some()
        || movement.order.is_some()
        || !posture_allowed
        || !movement.entry_effects.is_empty()
        || !matches!(
            &movement.to,
            MoveDestination::Owners {
                zone: OwnerZone::Hand | OwnerZone::Cemetery
            } | MoveDestination::Shared {
                zone: ZoneRef {
                    kind: ZoneKind::Ex | ZoneKind::Field,
                    ..
                }
            }
        )
    {
        return Err(unsupported("search movement"));
    }
    Ok(())
}

/// These capabilities still require an authorized selector spine at their use site.
pub(super) fn private_requirement(code: &str) -> bool {
    matches!(
        code,
        "shape.Action.Search"
            | "action.search"
            | "shape.SearchDestination.Single"
            | "shape.SearchDestination.Partition"
            | "shape.SearchDestination.OptionalSelfReplacement"
            | "shape.Predicate.SearchFoundAny"
            | "shape.Predicate.NameContains"
            | "predicate.name-contains"
            | "shape.Predicate.NamesIntersect"
            | "predicate.names-intersect"
            | "shape.NamesRead.Names"
            | "read.name"
            | "shape.ObjectOperand.Bound"
            | "shape.Predicate.HasAbility"
            | "predicate.has-ability"
            | "shape.AbilitiesRead.Abilities"
            | "read.abilities"
            | "shape.Predicate.HasTrait"
            | "predicate.has-trait"
            | "shape.TraitsRead.Traits"
            | "read.traits"
            | "shape.Predicate.KindEquals"
            | "predicate.kind-equals"
            | "shape.Predicate.ClassEquals"
            | "predicate.class-equals"
            | "shape.ClassRead.Class"
            | "read.affiliation.class"
            | "shape.Info.Present"
            | "shape.BoolRead.State"
            | "read.state"
            | "internal.BooleanState.TokenStatus"
            | "shape.KindRead.Kind"
            | "read.kind"
            | "shape.Predicate.PostureEquals"
            | "predicate.posture-equals"
            | "shape.PostureRead.Posture"
            | "read.state.posture"
            | "internal.CharacteristicView.Original"
            | "internal.KeywordId.Evolution"
            | "internal.CardKind.Follower"
            | "internal.CardKind.Spell"
            | "internal.CardKind.Amulet"
            | "shape.MoveDestination.Shared"
            | "internal.OwnerZone.Hand"
            | "internal.Disjointness.Disjoint"
            | "internal.ObjectProperty.Names"
    )
}

// Grants and characteristic predicates share identifiers, but their checked owners differ.
pub(super) fn keyword_requirement(
    requirements: &Requirements,
    requirement: &Requirement,
    origins: &BTreeSet<String>,
) -> bool {
    if !matches!(
        requirement.capability.code,
        "internal.KeywordId.Ward"
            | "internal.KeywordId.Aura"
            | "internal.KeywordId.Storm"
            | "internal.KeywordId.Rush"
            | "internal.KeywordId.Bane"
            | "internal.KeywordId.Intimidate"
            | "internal.KeywordId.Assail"
            | "internal.KeywordId.Stack"
            | "internal.KeywordId.EarthRite"
            | "internal.KeywordId.Lastword"
    ) {
        return false;
    }
    requirement.locations.iter().all(|location| {
        origins.contains(&location.ir_path)
            || requirements
                .required
                .iter()
                .filter(|owner| owner.capability.code == "read.zone-count")
                .flat_map(|owner| &owner.locations)
                .any(|owner| {
                    location
                        .ir_path
                        .starts_with(&format!("{}/selector/", owner.ir_path))
                })
            || requirements
                .required
                .iter()
                .filter(|owner| {
                    matches!(
                        owner.capability.code,
                        "shape.Action.Search" | "shape.Predicate.HasAbility"
                    )
                })
                .flat_map(|owner| &owner.locations)
                .any(|owner| location.ir_path.starts_with(&format!("{}/", owner.ir_path)))
    })
}

fn bounds(printed: Cardinality, available: usize, conditional: bool) -> Result<Cardinality> {
    let available = u64::try_from(available).map_err(invalid)?;
    Ok(Cardinality {
        minimum: Nat(if conditional {
            0
        } else {
            printed.minimum.0.min(available)
        }),
        maximum: Some(Nat(printed
            .maximum
            .map_or(available, |n| n.0.min(available)))),
    })
}

pub(super) fn matches_keyword(game: &Game, id: &str, keyword: KeywordId) -> Result<bool> {
    game.require_internal_abilities(id, "search keyword")?;
    if matches!(keyword, KeywordId::EarthRite | KeywordId::Lastword) {
        return keywords::matches(game, id, keyword);
    }
    if let Some(name) = super::grant::keyword(keyword)
        && game.object(id)?.zone == "field"
    {
        return Ok(game.keywords(id)?.contains(name));
    }
    Ok(game
        .internal_characteristic_abilities(id)?
        .iter()
        .any(|ability| {
            printed_keywords::card_declaration(&ability.node) == Some(keyword)
                || (keyword == KeywordId::Evolution
                    && matches!(
                        ability.node.source.origin,
                        AbilityOrigin::Keyword {
                            keyword: KeywordId::Evolution,
                            ..
                        }
                    ))
        }))
}

pub(super) fn matches_candidate(
    game: &Game,
    id: &str,
    condition: &super::conditions::PreparedCondition,
) -> Result<bool> {
    super::conditions::candidate::matches(condition, game, id)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Search shares authoritative movement and shuffle primitives."
)]
impl Game {
    #[expect(
        clippy::too_many_lines,
        reason = "All private domains and constraints are frozen before the one search question."
    )]
    pub(super) fn internal_search(
        &mut self,
        execution: &mut Execution,
        instruction: Box<Instruction>,
    ) -> Result<()> {
        let Action::Search {
            decision,
            player,
            deck: deck_player,
            candidate,
            filter,
            cardinality,
            groups,
            destination,
            ..
        } = &instruction.action
        else {
            return Err(unsupported("search action"));
        };
        let player = execution.player(self, player)?;
        if execution.player(self, deck_player)? != player {
            return Err(unsupported("searching another player's deck"));
        }
        let seat = seat_name(player)?;
        let deck = self.internal_known_deck(seat)?;
        match destination {
            Some(
                SearchDestination::Single { movement }
                | SearchDestination::OptionalSelfReplacement { movement, .. },
            ) => {
                self.internal_search_destination(execution, &movement.to, player)?;
            }
            Some(SearchDestination::Partition { .. }) => {}
            _ => return Err(unsupported("search destination")),
        }
        let plan = super::conditions::ConditionPlan::compile(
            filter,
            *candidate,
            super::conditions::Policy::Search,
        )?;
        let conditional = plan.conditional() || groups.is_some();
        let condition = plan.prepare(Some(&super::conditions::candidate::Operands {
            game: self,
            execution,
        }))?;
        let mut members = Vec::new();
        for id in &deck {
            let object = self.object(id)?;
            if object.owner != seat || object.controller != seat || object.zone != "deck" {
                return Err(unsupported("search deck membership"));
            }
            if matches_candidate(self, id, &condition)? {
                members.push(execution.reference(self, id)?);
            }
        }
        // Handles describe a set, not the deck's hidden slot order or a destination permutation.
        members.sort_by_key(|object| object.id);
        let cards = members
            .iter()
            .enumerate()
            .map(|(index, &object)| {
                let id = execution.object_id(object)?;
                Ok(OrderCard {
                    handle: u64::try_from(index).map_err(invalid)?,
                    card: self.object(id)?.card.clone(),
                    name: self.card_name(id)?,
                    object: None,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let (group_members, domain_groups) = if let Some(constraint) = groups {
            let mut domains = Vec::new();
            let mut views = Vec::new();
            for group in &constraint.groups {
                let Selector::Zone {
                    binding, predicate, ..
                } = &group.selector
                else {
                    return Err(unsupported("search group"));
                };
                let predicate = super::conditions::ConditionPlan::compile(
                    predicate,
                    *binding,
                    super::conditions::Policy::Search,
                )?
                .prepare(Some(&super::conditions::candidate::Operands {
                    game: self,
                    execution,
                }))?;
                let mut domain = Vec::new();
                let mut handles = Vec::new();
                for (index, &object) in members.iter().enumerate() {
                    if matches_candidate(self, execution.object_id(object)?, &predicate)? {
                        domain.push(object);
                        handles.push(u64::try_from(index).map_err(invalid)?);
                    }
                }
                views.push(SearchGroup {
                    handles,
                    cardinality: bounds(group.cardinality, domain.len(), true)?,
                });
                domains.push(domain);
            }
            (Some(domains), Some(views))
        } else {
            (None, None)
        };
        let cardinality = bounds(*cardinality, deck.len(), conditional)?;
        let domain = SearchDomain {
            cards,
            cardinality,
            groups: domain_groups,
        };
        let decision = *decision;
        let distinct_names = groups
            .as_ref()
            .is_some_and(|groups| groups.distinct_by == Some(ObjectProperty::Names));
        let cost_cap = match groups
            .as_ref()
            .map(|groups| card_totals(&groups.aggregate_bounds))
        {
            None | Some(CardTotals::Unbounded) => None,
            Some(CardTotals::OriginalCostAtMost(limit)) => Some(limit),
            Some(CardTotals::Unsupported) => return Err(unsupported("search aggregate bound")),
        };
        let names = if distinct_names {
            members
                .iter()
                .map(|member| self.card_names(execution.object_id(*member)?))
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![]
        };
        let wait = SearchWait {
            instruction,
            player,
            deck,
            members,
            group_members,
            domain: domain.clone(),
            snapshot: SnapshotId(self.state.ir_epoch),
            conditional,
            distinct_names,
            names,
            cost_cap,
        };
        self.internal_wait(
            execution,
            decision,
            player,
            InputDomain::Search(domain),
            Wait::Search(wait),
        )
    }

    /// Distinct names and the original cost cap span every found card.
    fn internal_search_check_totals(
        &self,
        execution: &Execution,
        wait: &SearchWait,
        found: &[ObjectRef],
    ) -> Result<()> {
        let mut names = DistinctProperties::default();
        for object in found {
            let index = wait
                .members
                .iter()
                .position(|member| member == object)
                .ok_or_else(|| invalid("foreign search member"))?;
            if wait.distinct_names {
                let own = wait
                    .names
                    .get(index)
                    .ok_or_else(|| invalid("missing frozen search names"))?;
                if !names.insert_member(own.iter().map(|name| PropertyKey::Name(name))) {
                    return Err(invalid("search names must differ"));
                }
            }
        }
        if let Some(limit) = wait.cost_cap
            && !within_original_cost_cap(
                found.iter().map(|object| {
                    let id = execution.object_id(*object)?;
                    super::cost::printed_cost(&self.internal_cost_print(id)?["cost"])
                }),
                limit,
                "search cost sum overflow",
            )?
        {
            return Err(invalid("search original cost total exceeds its cap"));
        }
        Ok(())
    }

    pub(super) fn internal_search_validate(
        &self,
        execution: &Execution,
        wait: &SearchWait,
        answer: SearchAnswer,
    ) -> Result<Vec<ObjectRef>> {
        let translate = |handles: Vec<u64>| {
            handles
                .into_iter()
                .map(|handle| {
                    wait.members
                        .get(usize::try_from(handle).map_err(invalid)?)
                        .copied()
                        .ok_or_else(|| invalid("foreign search handle"))
                })
                .collect::<Result<Vec<_>>>()
        };
        let answer = match answer {
            SearchAnswer::Ungrouped(handles) => SearchSelectionAnswer::Ungrouped {
                cards: translate(handles)?,
            },
            SearchAnswer::Grouped(groups) => SearchSelectionAnswer::Grouped {
                groups: groups
                    .into_iter()
                    .map(translate)
                    .collect::<Result<Vec<_>>>()?,
            },
        };
        if !answer.matches_domain(&wait.members, wait.group_members.as_deref()) {
            return Err(invalid("search answer does not match its frozen domain"));
        }
        let mut found = match &answer {
            SearchSelectionAnswer::Ungrouped { cards } => cards.clone(),
            SearchSelectionAnswer::Grouped { groups } => {
                let views = wait
                    .domain
                    .groups
                    .as_ref()
                    .ok_or_else(|| invalid("missing search groups"))?;
                for (cards, view) in groups.iter().zip(views) {
                    check_count(view.cardinality, cards.len(), "search group cardinality")?;
                }
                groups.iter().flatten().copied().collect()
            }
        };
        check_count(
            wait.domain.cardinality,
            found.len(),
            "search total cardinality",
        )?;
        self.internal_search_check_totals(execution, wait, &found)?;
        if let Action::Search {
            destination: Some(SearchDestination::Single { movement }),
            ..
        } = &wait.instruction.action
        {
            self.internal_search_check_move(execution, &found, &movement.to, wait.player)?;
        }
        if let Action::Search {
            destination:
                Some(SearchDestination::OptionalSelfReplacement {
                    predicate,
                    movement,
                    ..
                }),
            ..
        } = &wait.instruction.action
        {
            self.internal_search_replacement_validate(
                execution,
                &found,
                predicate,
                &movement.to,
                wait.player,
            )?;
        }
        found.sort_by_key(|object| object.id);
        Ok(found)
    }

    pub(super) fn internal_search_answer(
        &mut self,
        execution: &mut Execution,
        wait: &SearchWait,
        found: Vec<ObjectRef>,
    ) -> Result<()> {
        for &object in &found {
            if self.object(execution.object_id(object)?)?.generation != object.generation.0 {
                return Err(invalid("stale search generation"));
            }
        }
        let seat = seat_name(wait.player)?;
        if self.internal_known_deck(seat)? != wait.deck
            || self.state.ir_epoch
                != wait
                    .snapshot
                    .0
                    .checked_add(1)
                    .ok_or_else(|| invalid("search epoch overflow"))?
        {
            return Err(invalid("search snapshot changed"));
        }
        let views = found
            .iter()
            .map(|&object| RuntimeObjectView {
                object,
                snapshot: wait.snapshot,
            })
            .collect::<Vec<_>>();
        let ids = found
            .iter()
            .map(|&object| execution.object_id(object).map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        if wait.conditional {
            self.reveal(&ids, "all", &execution.frame(), false)?;
        }
        let outcome = SearchOutcome {
            found: views.clone(),
            revealed: if wait.conditional { views } else { Vec::new() },
            moved: Vec::new(),
        };
        match &wait.instruction.action {
            Action::Search {
                destination: Some(SearchDestination::Single { movement }),
                ..
            } => {
                if let Some(partition) = self.internal_search_capacity(
                    execution,
                    &movement.to,
                    wait.player,
                    found.len(),
                )? {
                    return self.internal_search_partition(
                        execution,
                        PartitionContinuation {
                            instruction: wait.instruction.clone(),
                            player: wait.player,
                            outcome,
                            remaining: found,
                            destinations: VecDeque::from([partition]),
                            next_handle: u64::try_from(wait.domain.cards.len()).map_err(invalid)?,
                            capacity_remainder: Some(movement.to.clone()),
                            entry_posture: movement.initial_posture,
                        },
                    );
                }
                let moved = self.internal_search_move(
                    execution,
                    &found,
                    &movement.to,
                    wait.player,
                    movement.initial_posture,
                )?;
                self.internal_search_finish(
                    execution,
                    &wait.instruction,
                    wait.player,
                    SearchOutcome { moved, ..outcome },
                )
            }
            Action::Search {
                destination: Some(SearchDestination::Partition { destinations }),
                ..
            } => self.internal_search_partition(
                execution,
                PartitionContinuation {
                    instruction: wait.instruction.clone(),
                    player: wait.player,
                    outcome,
                    remaining: found,
                    destinations: destinations.clone().into(),
                    next_handle: u64::try_from(wait.domain.cards.len()).map_err(invalid)?,
                    capacity_remainder: None,
                    entry_posture: None,
                },
            ),
            Action::Search {
                destination: Some(SearchDestination::OptionalSelfReplacement { .. }),
                ..
            } => {
                self.internal_search_replacement(execution, &wait.instruction, wait.player, outcome)
            }
            _ => Err(unsupported("search movement continuation")),
        }
    }

    fn internal_search_capacity(
        &self,
        execution: &Execution,
        to: &MoveDestination,
        player: PlayerId,
        count: usize,
    ) -> Result<Option<SearchPartition>> {
        let (zone, kind) = self.internal_search_destination(execution, to, player)?;
        if !matches!(kind, ZoneKind::Ex | ZoneKind::Field) {
            return Ok(None);
        }
        let free = usize::try_from(
            5_i64
                .saturating_sub(self.zone_count(seat_name(player)?, zone))
                .max(0),
        )
        .map_err(invalid)?;
        if free == 0 || count <= free {
            return Ok(None);
        }
        let available = Nat(u64::try_from(free).map_err(invalid)?);
        Ok(Some(SearchPartition {
            decision: DecisionId(0),
            zone: to.clone(),
            cardinality: Cardinality {
                minimum: available,
                maximum: Some(available),
            },
        }))
    }

    fn internal_search_failed(
        &self,
        execution: &Execution,
        found: &[ObjectRef],
        to: &MoveDestination,
        player: PlayerId,
    ) -> Result<Vec<MoveResult>> {
        let (_, kind) = self.internal_search_destination(execution, to, player)?;
        Ok(found
            .iter()
            .map(|&before| MoveResult {
                before,
                after: None,
                requested_to: RuntimeZoneRef { player, kind },
                requested_position: None,
                actual_to: None,
                actual_position: None,
                replacements: Vec::new(),
            })
            .collect())
    }

    fn internal_search_finish(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        player: PlayerId,
        outcome: SearchOutcome,
    ) -> Result<()> {
        let seat = seat_name(player)?;
        let remaining = self.internal_known_deck(seat)?;
        self.shuffle(seat, &remaining)?;
        execution.look_viewers.clear();
        execution.facts.push(Fact::Shuffled { player });
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Search(outcome));
        Ok(())
    }

    fn internal_search_partition(
        &mut self,
        execution: &mut Execution,
        mut continuation: PartitionContinuation,
    ) -> Result<()> {
        while let Some(partition) = continuation.destinations.pop_front() {
            // Empty segments consume no token, but never skip a nonempty singleton question.
            if continuation.remaining.is_empty() {
                continue;
            }
            let mut cards = Vec::new();
            for &object in &continuation.remaining {
                let id = execution.object_id(object)?;
                let handle = continuation.next_handle;
                continuation.next_handle = handle
                    .checked_add(1)
                    .ok_or_else(|| invalid("search handle overflow"))?;
                cards.push(OrderCard {
                    handle,
                    card: self.object(id)?.card.clone(),
                    name: self.card_name(id)?,
                    object: None,
                });
            }
            let domain = SearchPartitionDomain {
                cards,
                cardinality: bounds(partition.cardinality, continuation.remaining.len(), false)?,
            };
            let player = continuation.player;
            let decision = partition.decision;
            let wait = PartitionWait {
                continuation,
                partition,
                domain: domain.clone(),
                deck: self.internal_known_deck(seat_name(player)?)?,
                snapshot: SnapshotId(self.state.ir_epoch),
            };
            return self.internal_wait(
                execution,
                decision,
                player,
                InputDomain::SearchPartition(domain),
                Wait::SearchPartition(wait),
            );
        }
        if let Some(to) = &continuation.capacity_remainder {
            continuation
                .outcome
                .moved
                .extend(self.internal_search_failed(
                    execution,
                    &continuation.remaining,
                    to,
                    continuation.player,
                )?);
        }
        self.internal_search_finish(
            execution,
            &continuation.instruction,
            continuation.player,
            continuation.outcome,
        )
    }

    pub(super) fn internal_search_partition_validate(
        &self,
        execution: &Execution,
        wait: &PartitionWait,
        handles: &[u64],
    ) -> Result<Vec<ObjectRef>> {
        let mut cards = handles
            .iter()
            .map(|handle| {
                let index = wait
                    .domain
                    .cards
                    .iter()
                    .position(|card| card.handle == *handle)
                    .ok_or_else(|| invalid("foreign partition handle"))?;
                Ok(wait.continuation.remaining[index])
            })
            .collect::<Result<Vec<_>>>()?;
        if !(SearchPartitionAnswer {
            cards: cards.clone(),
        })
        .matches_remainder(&wait.continuation.remaining, wait.partition.cardinality)
        {
            return Err(invalid("partition answer does not match its remainder"));
        }
        self.internal_search_check_move(
            execution,
            &cards,
            &wait.partition.zone,
            wait.continuation.player,
        )?;
        cards.sort_by_key(|object| object.id);
        Ok(cards)
    }

    pub(super) fn internal_search_partition_answer(
        &mut self,
        execution: &mut Execution,
        wait: &PartitionWait,
        cards: &[ObjectRef],
    ) -> Result<()> {
        if self.internal_known_deck(seat_name(wait.continuation.player)?)? != wait.deck
            || self.state.ir_epoch
                != wait
                    .snapshot
                    .0
                    .checked_add(1)
                    .ok_or_else(|| invalid("partition epoch overflow"))?
        {
            return Err(invalid("partition snapshot changed"));
        }
        for &object in &wait.continuation.remaining {
            if self.object(execution.object_id(object)?)?.generation != object.generation.0 {
                return Err(invalid("stale partition generation"));
            }
        }
        let mut continuation = wait.continuation.clone();
        // Allocation consumes members even if a future admitted movement can fail.
        continuation
            .remaining
            .retain(|object| !cards.contains(object));
        continuation.outcome.moved.extend(self.internal_search_move(
            execution,
            cards,
            &wait.partition.zone,
            continuation.player,
            continuation.entry_posture,
        )?);
        self.internal_search_partition(execution, continuation)
    }

    fn internal_search_destination(
        &self,
        execution: &Execution,
        to: &MoveDestination,
        player: PlayerId,
    ) -> Result<(&'static str, ZoneKind)> {
        match to {
            MoveDestination::Owners {
                zone: OwnerZone::Hand,
            } => Ok(("hand", ZoneKind::Hand)),
            MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            } => Ok(("cemetery", ZoneKind::Cemetery)),
            MoveDestination::Shared { zone }
                if execution.player(self, &zone.player)? == player
                    && matches!(zone.kind, ZoneKind::Field | ZoneKind::Ex) =>
            {
                Ok((zone_name(zone.kind)?, zone.kind))
            }
            _ => Err(unsupported("search destination owner")),
        }
    }

    fn internal_search_check_move(
        &self,
        execution: &Execution,
        found: &[ObjectRef],
        to: &MoveDestination,
        player: PlayerId,
    ) -> Result<()> {
        let (zone, kind) = self.internal_search_destination(execution, to, player)?;
        if kind == ZoneKind::Field && self.zone_count(seat_name(player)?, zone) < 5 {
            for &object in found {
                if self
                    .object_type(execution.object_id(object)?)?
                    .contains("スペル")
                {
                    return Err(unsupported("putting a found spell onto the field"));
                }
            }
        }
        Ok(())
    }

    pub(super) fn internal_search_move(
        &mut self,
        execution: &mut Execution,
        found: &[ObjectRef],
        to: &MoveDestination,
        player: PlayerId,
        initial_posture: Option<Posture>,
    ) -> Result<Vec<MoveResult>> {
        let (zone, kind) = self.internal_search_destination(execution, to, player)?;
        let seat = seat_name(player)?;
        if matches!(kind, ZoneKind::Ex | ZoneKind::Field) && self.zone_count(seat, zone) >= 5 {
            return self.internal_search_failed(execution, found, to, player);
        }
        if kind == ZoneKind::Field {
            for &before in found {
                if self
                    .object_type(execution.object_id(before)?)?
                    .contains("スペル")
                {
                    return Err(unsupported("putting a found spell onto the field"));
                }
            }
        }
        let (moved, _) = self.internal_move_members(
            execution,
            found,
            zone,
            kind,
            MoveMethod::Put,
            false,
            None,
            initial_posture,
        )?;
        if kind == ZoneKind::Hand {
            for movement in &moved {
                let id = execution.object_id(movement.before)?;
                for (viewer, knowledge) in &mut self.state.knowledge {
                    if viewer != seat {
                        knowledge.forget_location(id);
                    }
                }
            }
        }
        Ok(moved)
    }
}

fn zone_name(kind: ZoneKind) -> Result<&'static str> {
    match kind {
        ZoneKind::Field => Ok("field"),
        ZoneKind::Ex => Ok("ex"),
        _ => Err(unsupported("search capacity zone")),
    }
}
