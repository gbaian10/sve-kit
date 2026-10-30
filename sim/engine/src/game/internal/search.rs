//! Search owns its private candidate evaluator and never lends it to ordinary selectors.

use super::{
    Execution, Fact, Game, InputDomain, InstructionOutcome, OrderCard, Result, Wait, contains,
    invalid, seat_name, unsupported,
};
use crate::ir::printed_keywords;
use crate::ir::{
    AbilitiesRead, AbilityOrigin, AbilityPredicate, AbilityReference, Action, Binding, Bool,
    BoolRead, BooleanState, CardKind, CardName, Cardinality, CharacteristicView, ClassRead,
    Comparison, DecisionId, Expr, Info, Instruction, Int, IntOperation, KeywordId, KindRead,
    MoveDestination, MoveMethod, MoveResult, MoveSpec, NamesRead, Nat, NumberRead, ObjectOperand,
    ObjectProperty, ObjectRef, ObjectView, OwnerZone, PlayerId, PlayerRelation, Predicate,
    Requirement, Requirements, RuntimeObjectView, RuntimeZoneRef, SearchDestination, SearchOutcome,
    SearchPartition, SearchPartitionAnswer, SearchSelectionAnswer, SelectionView, Selector,
    SnapshotId, Stat, TraitsRead, ZoneKind, ZoneRef,
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
    capacity_remainder: Option<MoveDestination>,
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
pub(super) fn check_predicate(value: &Expr<Bool>, candidate: Binding<ObjectRef>) -> Result<bool> {
    match value {
        Expr::Literal(Bool(true)) => Ok(false),
        Expr::Live(read) if matches!(read.as_ref(), BoolRead::State { object, property: BooleanState::TokenStatus } if candidate_view(object, candidate)) => {
            Ok(true)
        }
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::ClassEquals {
                left: Expr::Live(read),
                right: Expr::Literal(Info::Present(class)),
            } if !class.0.is_empty()
                && matches!(read.as_ref(), ClassRead::Class { object, view: CharacteristicView::Original | CharacteristicView::Effective } if candidate_view(object, candidate)) =>
            {
                Ok(true)
            }
            Predicate::CompareInt {
                left: Expr::Compute(read),
                right: Expr::Literal(Int(amount)),
                ..
            } if *amount >= 0 && numeric_filter(read, candidate) => Ok(true),
            Predicate::HasAbility {
                abilities,
                predicate,
            } if keyword_filter(abilities, predicate, candidate) => Ok(true),
            Predicate::NameContains {
                object,
                name: Expr::Literal(name),
            } if candidate_view(object, candidate) && !name.0.is_empty() => Ok(true),
            Predicate::NamesIntersect {
                left: Expr::Live(read),
                right,
            } if matches!(read.as_ref(), NamesRead::Names { object } if candidate_view(object, candidate))
                && search_names(right) =>
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

fn search_names(names: &Expr<Vec<CardName>>) -> bool {
    match names {
        Expr::Literal(names) => !names.is_empty() && names.iter().all(|name| !name.0.is_empty()),
        Expr::Live(read) => matches!(
            read.as_ref(),
            NamesRead::Names {
                object: ObjectView::SourceInformation {}
            }
        ),
        _ => false,
    }
}

// Freeze the checked source through its existing authority before consulting hidden candidates.
fn freeze_source_names(
    game: &Game,
    execution: &Execution,
    value: &Expr<Bool>,
) -> Result<Expr<Bool>> {
    let Expr::Compute(operation) = value else {
        return Ok(value.clone());
    };
    let operation = match operation.as_ref() {
        Predicate::NamesIntersect {
            left,
            right: Expr::Live(read),
        } if matches!(
            read.as_ref(),
            NamesRead::Names {
                object: ObjectView::SourceInformation {}
            }
        ) =>
        {
            Predicate::NamesIntersect {
                left: left.clone(),
                right: Expr::Literal(execution.source_names(game)?),
            }
        }
        Predicate::Not { predicate } => Predicate::Not {
            predicate: freeze_source_names(game, execution, predicate)?,
        },
        Predicate::And { predicates } => Predicate::And {
            predicates: predicates
                .iter()
                .map(|p| freeze_source_names(game, execution, p))
                .collect::<Result<_>>()?,
        },
        Predicate::Or { predicates } => Predicate::Or {
            predicates: predicates
                .iter()
                .map(|p| freeze_source_names(game, execution, p))
                .collect::<Result<_>>()?,
        },
        other => other.clone(),
    };
    Ok(Expr::Compute(Box::new(operation)))
}

fn numeric_filter(read: &IntOperation, candidate: Binding<ObjectRef>) -> bool {
    matches!(read, IntOperation::InformationOrZero { information:
        NumberRead::Cost { object, view: CharacteristicView::Original | CharacteristicView::Effective }
        | NumberRead::Stats { object, view: CharacteristicView::Effective, property: Stat::Attack }
    } if candidate_view(object, candidate))
}

pub(super) fn keyword_filter(
    abilities: &Expr<Vec<AbilityReference>>,
    predicate: &AbilityPredicate,
    candidate: Binding<ObjectRef>,
) -> bool {
    matches!(abilities, Expr::Live(read)
        if matches!(read.as_ref(), AbilitiesRead::Abilities { object, view: CharacteristicView::Effective }
            if candidate_view(object, candidate)))
        && predicate.kind.is_none()
        && predicate.except.is_empty()
        && predicate.holder.is_none()
        && predicate.origin.is_none()
        && (predicate.keyword == Some(KeywordId::Evolution)
            || predicate.keyword.and_then(super::grant::keyword).is_some())
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

// Keyword identifiers are also used by grants; only their Search-owned locations are private reads.
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
    ) {
        return false;
    }
    requirement.locations.iter().all(|location| {
        origins.contains(&location.ir_path)
            || requirements
                .required
                .iter()
                .filter(|owner| owner.capability.code == "shape.Action.Search")
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

fn matches_number(
    game: &Game,
    id: &str,
    read: &IntOperation,
    comparison: Comparison,
    amount: i64,
) -> Result<bool> {
    let IntOperation::InformationOrZero { information } = read else {
        return Err(unsupported("search number operation"));
    };
    let actual = match information {
        NumberRead::Cost {
            view: CharacteristicView::Original,
            ..
        } => super::cost::printed_cost(&game.internal_cost_print(id)?["cost"])?,
        NumberRead::Cost {
            view: CharacteristicView::Effective,
            ..
        } => game.internal_effective_cost(id)?,
        NumberRead::Stats {
            view: CharacteristicView::Effective,
            property: Stat::Attack,
            ..
        } => {
            if game.object_type(id)?.contains("フォロワー") {
                game.object(id)?.state["power"]
                    .as_i64()
                    .ok_or_else(|| invalid("search candidate power"))?
            } else {
                0
            }
        }
        _ => return Err(unsupported("search numeric characteristic")),
    };
    Ok(super::values::compare(actual, comparison, amount))
}

pub(super) fn matches_keyword(game: &Game, id: &str, predicate: &AbilityPredicate) -> Result<bool> {
    let keyword = predicate
        .keyword
        .ok_or_else(|| unsupported("search keyword"))?;
    game.require_internal_abilities(id, "search keyword")?;
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

pub(super) fn matches_candidate(game: &Game, id: &str, expression: &Expr<Bool>) -> Result<bool> {
    match expression {
        Expr::Literal(Bool(true)) => Ok(true),
        Expr::Live(read)
            if matches!(
                read.as_ref(),
                BoolRead::State {
                    property: BooleanState::TokenStatus,
                    ..
                }
            ) =>
        {
            Ok(game
                .face(id)?
                .get("card_type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.contains("トークン")))
        }
        Expr::Compute(operation) => {
            match operation.as_ref() {
                Predicate::ClassEquals {
                    right: Expr::Literal(Info::Present(class)),
                    ..
                } => Ok(game.face(id)?.get("card_class").and_then(Value::as_str)
                    == Some(class.0.as_str())),
                Predicate::CompareInt {
                    left: Expr::Compute(read),
                    right: Expr::Literal(Int(amount)),
                    comparison,
                } => matches_number(game, id, read, *comparison, *amount),
                Predicate::HasAbility { predicate, .. } => matches_keyword(game, id, predicate),
                Predicate::NameContains {
                    name: Expr::Literal(name),
                    ..
                } => Ok(game.card_name(id)?.contains(&name.0)),
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
            }
        }
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
        let conditional = check_predicate(filter, *candidate)? || groups.is_some();
        let filter = freeze_source_names(self, execution, filter)?;
        let mut members = Vec::new();
        for id in &deck {
            let object = self.object(id)?;
            if object.owner != seat || object.controller != seat || object.zone != "deck" {
                return Err(unsupported("search deck membership"));
            }
            if matches_candidate(self, id, &filter)? {
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
                let predicate = freeze_source_names(self, execution, predicate)?;
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
        for object in &found {
            let index = wait
                .members
                .iter()
                .position(|member| member == object)
                .ok_or_else(|| invalid("foreign search member"))?;
            let card = wait
                .domain
                .cards
                .get(index)
                .ok_or_else(|| invalid("missing frozen search card"))?;
            if wait.distinct_names && !names.insert(&card.name) {
                return Err(invalid("search names must differ"));
            }
        }
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
                        },
                    );
                }
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
                    capacity_remainder: None,
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
        let (moved, _) =
            self.internal_move_members(execution, found, zone, kind, MoveMethod::Put, false)?;
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
