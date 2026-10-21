//! Search owns its private candidate evaluator and never lends it to ordinary selectors.

use super::{
    Execution, Fact, Game, InputDomain, InstructionOutcome, OrderCard, Result, Wait, contains,
    invalid, seat_name, unsupported,
};
use crate::ir::{
    Action, Binding, Bool, CardKind, Cardinality, CharacteristicView, Expr, Instruction, KindRead,
    MoveDestination, MoveMethod, MoveResult, MoveSpec, NamesRead, Nat, ObjectOperand,
    ObjectProperty, ObjectRef, ObjectView, OwnerZone, PlayerId, PlayerRelation, Predicate,
    RuntimeObjectView, RuntimeZoneRef, SearchDestination, SearchOutcome, SearchPartition,
    SearchPartitionAnswer, SearchSelectionAnswer, SelectionView, Selector, SnapshotId, TraitsRead,
    ZoneKind, ZoneRef,
};
use alloc::collections::{BTreeSet, VecDeque};
use serde_json::{Value, json};

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

/// Frozen remaining found set for one printed destination allocation.
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
}

pub(super) fn packet(domain: &SearchDomain) -> Value {
    json!({"kind":"search","cards":domain.cards.iter().map(|card| json!({"handle":card.handle,"card":card.card,"name":card.name})).collect::<Vec<_>>(),
        "minimum":domain.cardinality.minimum.0,"maximum":domain.cardinality.maximum.map(|n| n.0),
        "groups":domain.groups.as_ref().map(|groups| groups.iter().map(|group| json!({"handles":group.handles,"minimum":group.cardinality.minimum.0,"maximum":group.cardinality.maximum.map(|n| n.0)})).collect::<Vec<_>>())})
}

// This closed grammar classifies conditions without consulting the hidden candidate population.
fn check_predicate(value: &Expr<Bool>, candidate: Binding<ObjectRef>) -> Result<bool> {
    match value {
        Expr::Literal(Bool(true)) => Ok(false),
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::NamesIntersect {
                left: Expr::Live(read),
                right: Expr::Literal(names),
            } if matches!(read.as_ref(), NamesRead::Names { object } if candidate_view(object, candidate))
                && !names.is_empty()
                && names.iter().all(|name| !name.0.is_empty()) =>
            {
                Ok(true)
            }
            Predicate::HasTrait {
                traits: Expr::Live(read),
                ..
            } if matches!(read.as_ref(), TraitsRead::Traits { object, view: CharacteristicView::Original | CharacteristicView::Effective } if candidate_view(object, candidate)) => {
                Ok(true)
            }
            Predicate::KindEquals {
                left: Expr::Live(read),
                right: Expr::Literal(_),
            } if matches!(read.as_ref(), KindRead::Kind { object, view: CharacteristicView::Original | CharacteristicView::Effective } if candidate_view(object, candidate)) => {
                Ok(true)
            }
            Predicate::Not { predicate } if check_predicate(predicate, candidate)? => Ok(true),
            Predicate::And { predicates } | Predicate::Or { predicates }
                if !predicates.is_empty() =>
            {
                for predicate in predicates {
                    if !check_predicate(predicate, candidate)? {
                        return Err(unsupported("ambiguous search condition"));
                    }
                }
                Ok(true)
            }
            _ => Err(unsupported("search predicate")),
        },
        _ => Err(unsupported("search predicate")),
    }
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
    let conditional = check_predicate(filter, *candidate)?;
    if let Some(groups) = groups {
        if !groups.aggregate_bounds.is_empty()
            || !matches!(groups.distinct_by, None | Some(ObjectProperty::Names))
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
    if movement.position.is_some()
        || movement.order.is_some()
        || movement.initial_posture.is_some()
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

/// Search-only requirement codes must never authorize private reads elsewhere in a program.
pub(super) fn private_requirement(code: &str) -> bool {
    matches!(
        code,
        "shape.Action.Search"
            | "action.search"
            | "shape.SearchDestination.Single"
            | "shape.SearchDestination.Partition"
            | "shape.SearchDestination.OptionalSelfReplacement"
            | "shape.Predicate.SearchFoundAny"
            | "shape.NumberRead.Cost"
            | "read.cost"
            | "shape.Predicate.NamesIntersect"
            | "predicate.names-intersect"
            | "shape.NamesRead.Names"
            | "read.name"
            | "shape.ObjectOperand.Bound"
            | "shape.Predicate.HasTrait"
            | "predicate.has-trait"
            | "shape.TraitsRead.Traits"
            | "read.traits"
            | "shape.Predicate.KindEquals"
            | "predicate.kind-equals"
            | "shape.KindRead.Kind"
            | "read.kind"
            | "internal.CharacteristicView.Original"
            | "internal.CardKind.Follower"
            | "internal.CardKind.Spell"
            | "internal.CardKind.Amulet"
            | "shape.MoveDestination.Shared"
            | "internal.OwnerZone.Hand"
            | "internal.Disjointness.Disjoint"
            | "internal.ObjectProperty.Names"
    )
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

fn matches_candidate(game: &Game, id: &str, expression: &Expr<Bool>) -> Result<bool> {
    match expression {
        Expr::Literal(Bool(true)) => Ok(true),
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::NamesIntersect {
                right: Expr::Literal(names),
                ..
            } => {
                let own = game.card_name(id)?;
                Ok(names.iter().any(|name| own == name.0))
            }
            Predicate::HasTrait { trait_id, .. } => Ok(game
                .face(id)?
                .get("traits")
                .and_then(Value::as_array)
                .is_some_and(|traits| {
                    traits
                        .iter()
                        .any(|value| value.as_str() == Some(trait_id.0.as_str()))
                })),
            Predicate::KindEquals {
                left: Expr::Live(read),
                right: Expr::Literal(kind),
            } => {
                let KindRead::Kind { view, .. } = read.as_ref();
                let kind_name = match view {
                    CharacteristicView::Original => game
                        .face(id)?
                        .get("card_type")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("missing printed kind"))?,
                    CharacteristicView::Effective => game.object_type(id)?,
                    _ => return Err(unsupported("search kind view")),
                };
                Ok(kind_name.contains(match kind {
                    CardKind::Follower => "フォロワー",
                    CardKind::Spell => "スペル",
                    CardKind::Amulet => "アミュレット",
                }))
            }
            Predicate::Not { predicate } => Ok(!matches_candidate(game, id, predicate)?),
            Predicate::And { predicates } => {
                for predicate in predicates {
                    if !matches_candidate(game, id, predicate)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Predicate::Or { predicates } => {
                for predicate in predicates {
                    if matches_candidate(game, id, predicate)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Err(unsupported("search predicate evaluation")),
        },
        _ => Err(unsupported("search predicate evaluation")),
    }
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
        let capacity_zone = match destination {
            Some(SearchDestination::Single { movement }) => {
                let zone = self
                    .internal_search_destination(execution, &movement.to, player)?
                    .1;
                matches!(zone, ZoneKind::Ex | ZoneKind::Field).then_some(zone)
            }
            Some(SearchDestination::Partition { .. }) => None,
            Some(SearchDestination::OptionalSelfReplacement { movement, .. }) => {
                self.internal_search_destination(execution, &movement.to, player)?;
                None
            }
            _ => return Err(unsupported("search destination")),
        };
        // A missing capacity protocol must not be replaced by asking the searcher to find fewer cards.
        if let Some(zone) = capacity_zone {
            let available = 5_i64
                .saturating_sub(self.zone_count(seat, zone_name(zone)?))
                .max(0);
            let upper = bounds(*cardinality, deck.len(), false)?
                .maximum
                .unwrap_or(Nat(0))
                .0;
            if available > 0 && upper > u64::try_from(available).map_err(invalid)? {
                return Err(unsupported("search needs a capacity selection"));
            }
        }
        let conditional = check_predicate(filter, *candidate)? || groups.is_some();
        let mut members = Vec::new();
        for id in &deck {
            let object = self.object(id)?;
            if object.owner != seat || object.controller != seat || object.zone != "deck" {
                return Err(unsupported("search deck membership"));
            }
            if matches_candidate(self, id, filter)? {
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
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let (group_members, domain_groups) = if let Some(constraint) = groups {
            let mut domains = Vec::new();
            let mut views = Vec::new();
            for group in &constraint.groups {
                let Selector::Zone { predicate, .. } = &group.selector else {
                    return Err(unsupported("search group"));
                };
                let mut domain = Vec::new();
                let mut handles = Vec::new();
                for (index, &object) in members.iter().enumerate() {
                    if matches_candidate(self, execution.object_id(object)?, predicate)? {
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
        };
        self.internal_wait(
            execution,
            decision,
            player,
            InputDomain::Search(domain),
            Wait::Search(wait),
        )
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Validation, proof, movement and final shuffle form one atomic answer transition."
    )]
    pub(super) fn internal_search_answer(
        &mut self,
        execution: &mut Execution,
        wait: &SearchWait,
        answer: SearchAnswer,
    ) -> Result<()> {
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
                    if !contains(
                        view.cardinality,
                        Nat(u64::try_from(cards.len()).map_err(invalid)?),
                    ) {
                        return Err(invalid("search group cardinality"));
                    }
                }
                groups.iter().flatten().copied().collect()
            }
        };
        if !contains(
            wait.domain.cardinality,
            Nat(u64::try_from(found.len()).map_err(invalid)?),
        ) {
            return Err(invalid("search total cardinality"));
        }
        let mut names = BTreeSet::new();
        for &object in &found {
            let id = execution.object_id(object)?;
            if self.object(id)?.generation != object.generation.0 {
                return Err(invalid("stale search generation"));
            }
            if wait.distinct_names && !names.insert(self.card_name(id)?) {
                return Err(invalid("search names must differ"));
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
        found.sort_by_key(|object| object.id);
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
            self.reveal(&ids, "all", &execution.frame(), false);
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
                let moved =
                    self.internal_search_move(execution, &found, &movement.to, wait.player)?;
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
        self.internal_search_finish(
            execution,
            &continuation.instruction,
            continuation.player,
            continuation.outcome,
        )
    }

    pub(super) fn internal_search_partition_answer(
        &mut self,
        execution: &mut Execution,
        wait: &PartitionWait,
        handles: &[u64],
    ) -> Result<()> {
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
        cards.sort_by_key(|object| object.id);
        let mut continuation = wait.continuation.clone();
        // Allocation consumes members even if a future admitted movement can fail.
        continuation
            .remaining
            .retain(|object| !cards.contains(object));
        continuation.outcome.moved.extend(self.internal_search_move(
            execution,
            &cards,
            &wait.partition.zone,
            continuation.player,
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

    pub(super) fn internal_search_move(
        &mut self,
        execution: &mut Execution,
        found: &[ObjectRef],
        to: &MoveDestination,
        player: PlayerId,
    ) -> Result<Vec<MoveResult>> {
        let (zone, kind) = self.internal_search_destination(execution, to, player)?;
        let seat = seat_name(player)?;
        if matches!(kind, ZoneKind::Ex | ZoneKind::Field) && self.zone_count(seat, zone) >= 5 {
            return Ok(found
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
                .collect());
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
        let (moved, _) =
            self.internal_move_members(execution, found, zone, kind, MoveMethod::Put)?;
        if kind == ZoneKind::Hand {
            for movement in &moved {
                let id = execution.object_id(movement.before)?;
                for (viewer, knowledge) in &mut self.state.knowledge {
                    if viewer != seat {
                        knowledge.located.remove(id);
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
