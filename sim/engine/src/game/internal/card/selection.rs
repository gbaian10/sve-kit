use super::super::{DecisionToken, DecisionView, InputDomain, Slot, Wait, contains};
use super::{Execution, Game, Nat, ObjectRef, Result, invalid, player_id, unsupported};
use crate::ir::effect_evolution;
use crate::ir::{
    Binding, BoolRead, BooleanState, CardKind, Cardinality, CharacteristicView, ChoiceDomain,
    ClassRead, Comparison, Decision, DecisionKind, Declinable, Disjointness, Info, Int,
    IntOperation, KindRead, MemberProjection, NamesRead, NumberRead, ObjectOperand, ObjectProperty,
    ObjectReceipt, ObjectView, PlayerRef, PlayerRelation, Predicate, ScopeId, SelectionConstraint,
    SelectionGroup, Selector, Stat, TraitsRead, Visibility,
};
use crate::ir::{Bool, Expr, SelectionView, ZoneKind};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use serde_json::{Value, json};

/// Object candidates visible only to the current chooser, addressed within that question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetDomain {
    /// Token-scoped objects; a discard may show the chooser's own hand cards.
    pub members: Vec<super::super::DistributionMember>,
    /// Bounds for this question; play-time targets retain their strict printed minimum.
    pub cardinality: Cardinality,
    /// Selected objects must not share a rules name.
    pub distinct_names: bool,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct TargetWait {
    pub(in crate::game::internal) decision: Decision,
    binding: Binding<Selector>,
    members: Vec<ObjectRef>,
    bounds: Cardinality,
    group_members: Vec<Vec<ObjectRef>>,
    group_bounds: Vec<Cardinality>,
    characteristics: Vec<(ObjectRef, TargetCharacteristics)>,
    distinct_names: bool,
    on_field_at_capture: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::game::internal) struct TargetCharacteristics {
    controller: String,
    zone: String,
    kind: String,
    traits: Vec<String>,
    names: Vec<String>,
}

pub(in crate::game::internal) fn characteristics(
    game: &Game,
    id: &str,
) -> Result<TargetCharacteristics> {
    // The admitted closure has no trait patches, static effects or base replacement during play.
    let traits = game.face(id)?["traits"]
        .as_array()
        .ok_or_else(|| invalid("missing effective traits"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("invalid effective trait"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(TargetCharacteristics {
        names: game.card_names(id)?,
        controller: game.object(id)?.controller.clone(),
        zone: game.object(id)?.zone.clone(),
        kind: game.object_type(id)?.into(),
        traits,
    })
}

impl TargetWait {
    fn is_private_look_member(&self, member: ObjectRef) -> bool {
        let ChoiceDomain::Objects {
            constraint,
            target_roles: None,
            ..
        } = &self.decision.domain
        else {
            return false;
        };
        if constraint.groups.is_empty() || constraint.groups.len() > 2 {
            return false;
        }
        let expected_bounds = Cardinality {
            minimum: Nat(0),
            maximum: u64::try_from(constraint.groups.len()).ok().map(Nat),
        };
        self.members.contains(&member)
            && constraint
                .groups
                .iter()
                .all(|group| look_filtered_selector(&group.selector))
            && constraint.groups.iter().all(|group| {
                group.cardinality.minimum == Nat(0) && group.cardinality.maximum == Some(Nat(1))
            })
            && constraint.cardinality == expected_bounds
            && self.decision.actor == (PlayerRef::Controller {})
            && matches!(
                &self.decision.visibility,
                Visibility::Only { players }
                    if players.relation == PlayerRelation::SelfPlayer
                        && players.relative_to == self.decision.actor
            )
            && matches!(
                self.decision.kind,
                DecisionKind::ResolutionChoice {
                    may: Declinable::MayDecline,
                    cardinality,
                    precondition: None,
                } if cardinality == expected_bounds
            )
    }

    pub(in crate::game::internal) fn check_current(
        &self,
        game: &Game,
        execution: &Execution,
        members: &[ObjectRef],
    ) -> Result<()> {
        for member in members {
            let id = execution.object_id(*member)?;
            let current = game.object(id)?;
            if current.generation != member.generation.0 {
                continue;
            }
            let saved = self
                .characteristics
                .iter()
                .find(|(reference, _)| reference == member)
                .ok_or_else(|| invalid("missing target selection producer"))?;
            if characteristics(game, id)? != saved.1 {
                return Err(unsupported("selected target characteristics changed"));
            }
        }
        Ok(())
    }

    pub(in crate::game::internal) fn validate(&self, handles: &[u64]) -> Result<Vec<ObjectRef>> {
        if !contains(
            self.bounds,
            Nat(u64::try_from(handles.len()).map_err(invalid)?),
        ) {
            return Err(invalid("target cardinality"));
        }
        let mut unique = BTreeSet::new();
        let members = handles
            .iter()
            .map(|handle| {
                if !unique.insert(*handle) {
                    return Err(invalid("duplicate target handle"));
                }
                self.members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("foreign target handle"))
            })
            .collect::<Result<Vec<_>>>()?;
        if !self.compatible(&members)? {
            return Err(invalid("target selection constraints"));
        }
        Ok(members)
    }

    fn compatible(&self, members: &[ObjectRef]) -> Result<bool> {
        if self.distinct_names {
            let mut names = BTreeSet::new();
            for member in members {
                let saved = self
                    .characteristics
                    .iter()
                    .find(|(reference, _)| reference == member)
                    .ok_or_else(|| invalid("missing target names"))?;
                let own: BTreeSet<_> = saved.1.names.iter().collect();
                if own.iter().any(|name| names.contains(name)) {
                    return Ok(false);
                }
                names.extend(own);
            }
        }
        // A single group is already bounded by `TargetWait::bounds`, whose minimum may be
        // lowered when a resolution has fewer available candidates than requested.
        if self.group_members.len() == 1 {
            return Ok(true);
        }
        Ok(assign_groups(
            members,
            0,
            &self.group_members,
            &self.group_bounds,
            &mut vec![0; self.group_members.len()],
        ))
    }

    fn capacity(&self) -> Result<u64> {
        if !self.distinct_names && self.group_members.len() <= 1 {
            return u64::try_from(self.members.len()).map_err(invalid);
        }
        if self.members.len() > 10 {
            return Err(unsupported("name selection exceeds both fields"));
        }
        let end = 1_u64
            .checked_shl(u32::try_from(self.members.len()).map_err(invalid)?)
            .ok_or_else(|| invalid("target plan overflow"))?;
        let mut maximum = 0;
        for mask in 0..end {
            let members = self
                .members
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1_u64 << index) != 0)
                .map(|(_, member)| *member)
                .collect::<Vec<_>>();
            if self.compatible(&members)? {
                maximum = maximum.max(u64::try_from(members.len()).map_err(invalid)?);
            }
        }
        Ok(maximum)
    }
}

fn assign_groups(
    members: &[ObjectRef],
    index: usize,
    group_members: &[Vec<ObjectRef>],
    bounds: &[Cardinality],
    counts: &mut [u64],
) -> bool {
    if index == members.len() {
        return bounds.iter().enumerate().all(|(group, bound)| {
            bound.minimum.0 <= counts[group]
                && bound
                    .maximum
                    .is_none_or(|maximum| counts[group] <= maximum.0)
        });
    }
    for group in 0..group_members.len() {
        if !group_members[group].contains(&members[index])
            || bounds[group]
                .maximum
                .is_some_and(|maximum| counts[group] >= maximum.0)
        {
            continue;
        }
        counts[group] = counts[group].saturating_add(1);
        if assign_groups(
            members,
            index.saturating_add(1),
            group_members,
            bounds,
            counts,
        ) {
            return true;
        }
        counts[group] = counts[group].saturating_sub(1);
    }
    false
}

fn private_look_decision(decision: &Decision, constraint: &SelectionConstraint) -> bool {
    matches!(
        &decision.visibility,
        Visibility::Only { players }
            if players.relation == PlayerRelation::SelfPlayer
                && players.relative_to == decision.actor
    ) && constraint
        .groups
        .iter()
        .all(|group| look_filtered_selector(&group.selector))
}

/// Candidates are the union of the groups; one candidate may belong to several groups.
fn group_layout(
    constraint: &SelectionConstraint,
    group_members: &[Vec<ObjectRef>],
) -> (Vec<ObjectRef>, Vec<Cardinality>, Cardinality) {
    let mut seen_members = BTreeSet::new();
    let members = group_members
        .iter()
        .flatten()
        .copied()
        .filter(|member| seen_members.insert((member.id, member.generation)))
        .collect();
    match constraint.groups.as_slice() {
        [group] => (
            members,
            vec![target_bounds(constraint, group)],
            target_bounds(constraint, group),
        ),
        groups => (
            members,
            groups.iter().map(|group| group.cardinality).collect(),
            constraint.cardinality,
        ),
    }
}

fn grouped_look_field_bounds(
    game: &Game,
    actor_seat: &str,
    observed_deck: bool,
    group_count: usize,
    mut bounds: Cardinality,
) -> Result<Cardinality> {
    if observed_deck && group_count > 1 {
        let free_field_slots = u64::try_from(
            5_i64
                .saturating_sub(game.zone_count(actor_seat, "field"))
                .max(0),
        )
        .map_err(invalid)?;
        bounds.maximum = Some(Nat(bounds
            .maximum
            .map_or(free_field_slots, |maximum| maximum.0.min(free_field_slots))));
    }
    Ok(bounds)
}

fn target_group(constraint: &SelectionConstraint) -> Result<&SelectionGroup> {
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("one selector group per target decision"));
    };
    if !matches!(constraint.distinct_by, None | Some(ObjectProperty::Names))
        || !constraint.aggregate_bounds.is_empty()
        || constraint.disjoint != Disjointness::Disjoint
    {
        return Err(unsupported("target properties or aggregate constraints"));
    }
    Ok(group)
}

fn target_bounds(constraint: &SelectionConstraint, group: &SelectionGroup) -> Cardinality {
    Cardinality {
        minimum: Nat(constraint
            .cardinality
            .minimum
            .0
            .max(group.cardinality.minimum.0)),
        maximum: constraint
            .cardinality
            .maximum
            .into_iter()
            .chain(group.cardinality.maximum)
            .min_by_key(|n| n.0),
    }
}

pub(super) fn check(constraint: &SelectionConstraint) -> Result<()> {
    field_selector(&target_group(constraint)?.selector)
}

pub(in crate::game::internal) fn check_resolution(decision: &Decision) -> Result<()> {
    if effect_evolution::is_choice(decision) {
        return Ok(());
    }
    if let ChoiceDomain::Recipients {
        candidates,
        cardinality,
        ..
    } = &decision.domain
    {
        if decision.actor != (PlayerRef::Controller {}) {
            return Err(unsupported("resolution recipient actor"));
        }
        return super::recipient_targets::check(candidates, *cardinality);
    }
    let ChoiceDomain::Objects {
        constraint,
        target_roles: None,
        ..
    } = &decision.domain
    else {
        return Err(unsupported("resolution public choice domain"));
    };
    let private_look_groups = !constraint.groups.is_empty()
        && constraint
            .groups
            .iter()
            .all(|group| private_look_filter(&group.selector).is_some());
    if private_look_groups {
        if decision.actor != (PlayerRef::Controller {})
            || constraint.groups.len() > 2
            || constraint.cardinality.minimum != Nat(0)
            || constraint
                .cardinality
                .maximum
                .is_none_or(|maximum| maximum.0 > 2)
            || constraint.distinct_by.is_some()
            || !constraint.aggregate_bounds.is_empty()
            || constraint.disjoint != Disjointness::Disjoint
        {
            return Err(unsupported("private observed choice actor or bounds"));
        }
        for group in &constraint.groups {
            if group.cardinality.minimum != Nat(0) || group.cardinality.maximum != Some(Nat(1)) {
                return Err(unsupported("private observed group bounds"));
            }
            let (binding, predicate) = private_look_filter(&group.selector)
                .ok_or_else(|| unsupported("private observed group selector"))?;
            check_predicate(predicate, binding)?;
        }
        return Ok(());
    }
    let group = target_group(constraint)?;
    match &group.selector {
        Selector::Zone {
            zone: ZoneKind::Ex,
            view: SelectionView::Current {},
            players,
            binding,
            predicate,
        } if players.relative_to == (PlayerRef::Controller {}) => {
            check_predicate(predicate, *binding)?;
        }
        Selector::Zone {
            zone: ZoneKind::Cemetery,
            view: SelectionView::Current {},
            players,
            binding,
            predicate,
        } if players.relative_to == (PlayerRef::Controller {})
            && players.relation == PlayerRelation::SelfPlayer =>
        {
            check_predicate(predicate, *binding)?;
        }
        selector => field_selector(selector)?,
    }
    let selector = match &group.selector {
        Selector::Difference { input, .. } => input.as_ref(),
        selector => selector,
    };
    if decision.actor != (PlayerRef::Controller {})
        || constraint.cardinality.maximum.is_none()
        || !matches!(
            selector,
            Selector::Zone {
                zone: ZoneKind::Field | ZoneKind::Ex | ZoneKind::Cemetery,
                ..
            }
        )
    {
        return Err(unsupported("resolution public choice actor or selector"));
    }
    Ok(())
}

pub(in crate::game::internal) fn field_selector(selector: &Selector) -> Result<()> {
    match selector {
        Selector::Zone {
            zone: ZoneKind::Field,
            view: SelectionView::Current {},
            predicate,
            binding,
            players,
        } if players.relative_to == (PlayerRef::Controller {}) => {
            check_predicate(predicate, *binding)
        }
        Selector::Difference { input, except } if matches!(except.as_ref(), Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}]) => {
            field_selector(input)
        }
        Selector::Captured { .. } | Selector::Bound { .. } | Selector::Objects { .. } => Ok(()),
        Selector::Union { sets } => {
            for set in sets {
                field_selector(set)?;
            }
            Ok(())
        }
        _ => Err(unsupported("public field target selector")),
    }
}

pub(in crate::game::internal) fn public_zone_selector(selector: &Selector) -> Result<()> {
    match selector {
        Selector::Zone {
            zone: ZoneKind::Field | ZoneKind::Cemetery,
            view: SelectionView::Current {},
            predicate,
            binding,
            players,
        } if players.relative_to == (PlayerRef::Controller {}) => {
            check_predicate(predicate, *binding)
        }
        _ => Err(unsupported("public count selector")),
    }
}

fn candidate_view(view: &ObjectView, candidate: Binding<ObjectRef>) -> bool {
    matches!(view, ObjectView::Current { object } if matches!(object.as_ref(), ObjectOperand::Bound { binding } if *binding == candidate))
}

fn candidate_number(read: &NumberRead, candidate: Binding<ObjectRef>) -> bool {
    match read {
        NumberRead::Cost {
            object,
            view: CharacteristicView::Original | CharacteristicView::Effective,
        }
        | NumberRead::Stats {
            object,
            view: CharacteristicView::Effective,
            property: Stat::Life,
        } => candidate_view(object, candidate),
        _ => false,
    }
}

fn private_look_filter(selector: &Selector) -> Option<(Binding<ObjectRef>, &Expr<Bool>)> {
    let Selector::Filter {
        input,
        binding,
        predicate,
    } = selector
    else {
        return None;
    };
    matches!(
        input.as_ref(),
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Look { .. },
            projection: MemberProjection::Before,
        }
    )
    .then_some((*binding, predicate))
}

fn check_predicate(value: &Expr<Bool>, candidate: Binding<ObjectRef>) -> Result<()> {
    match value {
        Expr::Literal(Bool(true)) => Ok(()),
        Expr::Live(read) if matches!(read.as_ref(), BoolRead::State { object, property: BooleanState::TokenStatus } if candidate_view(object, candidate)) => {
            Ok(())
        }
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::ClassEquals {
                left: Expr::Live(read),
                right: Expr::Literal(Info::Present(class)),
            } if !class.0.is_empty()
                && matches!(read.as_ref(), ClassRead::Class { object, view: CharacteristicView::Effective } if candidate_view(object, candidate)) =>
            {
                Ok(())
            }
            Predicate::KindEquals {
                left: Expr::Live(read),
                right: Expr::Literal(_),
            } if matches!(read.as_ref(), KindRead::Kind { object, view: CharacteristicView::Effective } if candidate_view(object, candidate)) => {
                Ok(())
            }
            Predicate::NamesIntersect {
                left: Expr::Live(read),
                right: Expr::Literal(names),
            } if !names.is_empty()
                && names.iter().all(|name| !name.0.is_empty())
                && matches!(read.as_ref(), NamesRead::Names { object } if candidate_view(object, candidate)) =>
            {
                Ok(())
            }
            Predicate::NameContains {
                object,
                name: Expr::Literal(name),
            } if !name.0.is_empty() && candidate_view(object, candidate) => Ok(()),
            Predicate::HasTrait {
                traits: Expr::Live(read),
                trait_id,
            } if !trait_id.0.is_empty()
                && matches!(read.as_ref(), TraitsRead::Traits { object, view: CharacteristicView::Effective } if candidate_view(object, candidate)) =>
            {
                Ok(())
            }
            Predicate::CompareInt {
                left: Expr::Compute(left),
                comparison: Comparison::Equal | Comparison::LessEqual | Comparison::GreaterEqual,
                right: Expr::Literal(Int(_)),
            } if matches!(left.as_ref(), IntOperation::InformationOrZero { information } if candidate_number(information, candidate)) => {
                Ok(())
            }
            Predicate::And { predicates } | Predicate::Or { predicates }
                if !predicates.is_empty() =>
            {
                for predicate in predicates {
                    check_predicate(predicate, candidate)?;
                }
                Ok(())
            }
            Predicate::Not { predicate } => check_predicate(predicate, candidate),
            _ => Err(unsupported(
                "unstable or unsupported field target predicate",
            )),
        },
        _ => Err(unsupported("field target predicate")),
    }
}

pub(super) fn matches_candidate(game: &Game, id: &str, value: &Expr<Bool>) -> Result<bool> {
    match value {
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
            Ok(game.object_type(id)?.contains("トークン"))
        }
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::ClassEquals {
                right: Expr::Literal(Info::Present(class)),
                ..
            } => {
                Ok(game.face(id)?.get("card_class").and_then(Value::as_str)
                    == Some(class.0.as_str()))
            }
            Predicate::KindEquals {
                right: Expr::Literal(kind),
                ..
            } => Ok(game.object_type(id)?.contains(match kind {
                CardKind::Follower => "フォロワー",
                CardKind::Spell => "スペル",
                CardKind::Amulet => "アミュレット",
            })),
            Predicate::NamesIntersect {
                right: Expr::Literal(names),
                ..
            } => Ok(game
                .card_names(id)?
                .iter()
                .any(|actual| names.iter().any(|name| &name.0 == actual))),
            Predicate::NameContains {
                name: Expr::Literal(name),
                ..
            } => Ok(game
                .card_names(id)?
                .iter()
                .any(|actual| actual.contains(&name.0))),
            Predicate::HasTrait { trait_id, .. } => {
                Ok(characteristics(game, id)?.traits.contains(&trait_id.0))
            }
            Predicate::CompareInt {
                left: Expr::Compute(left),
                comparison,
                right: Expr::Literal(Int(limit)),
            } => {
                let IntOperation::InformationOrZero { information } = left.as_ref() else {
                    return Err(unsupported("field target number"));
                };
                let actual = match information {
                    NumberRead::Cost {
                        view: CharacteristicView::Original,
                        ..
                    } => super::super::cost::printed_cost(&game.internal_cost_print(id)?["cost"])?,
                    NumberRead::Cost {
                        view: CharacteristicView::Effective,
                        ..
                    } => game.internal_effective_cost(id)?,
                    NumberRead::Stats {
                        view: CharacteristicView::Effective,
                        property,
                        ..
                    } => {
                        if game.object(id)?.zone != "field"
                            || !game.object_type(id)?.contains("フォロワー")
                        {
                            return Ok(false);
                        }
                        game.internal_numeric_stat(id, *property)?
                    }
                    _ => return Err(unsupported("field target characteristic")),
                };
                Ok(super::super::values::compare(actual, *comparison, *limit))
            }
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                let values = predicates
                    .iter()
                    .map(|predicate| matches_candidate(game, id, predicate))
                    .collect::<Result<Vec<_>>>()?;
                Ok(if matches!(operation.as_ref(), Predicate::And { .. }) {
                    values.iter().all(|matched| *matched)
                } else {
                    values.iter().any(|matched| *matched)
                })
            }
            Predicate::Not { predicate } => Ok(!matches_candidate(game, id, predicate)?),
            _ => Err(unsupported("field target predicate evaluation")),
        },
        _ => Err(unsupported("field target predicate evaluation")),
    }
}

pub(in crate::game::internal) fn packet(domain: &TargetDomain) -> Value {
    json!({"kind":"targets","distinct_names":domain.distinct_names,"minimum":domain.cardinality.minimum.0,"maximum":domain.cardinality.maximum.map(|n| n.0),"members":domain.members.iter().map(|member| json!({"handle":member.handle,"player":member.player.seat,"card":member.card,"name":member.name})).collect::<Vec<_>>()})
}

fn private_observed_deck(game: &Game, id: &str, actor_seat: &str, enabled: bool) -> bool {
    enabled
        && game.object(id).is_ok_and(|object| {
            object.zone == "deck" && object.owner == actor_seat && object.controller == actor_seat
        })
        && game
            .state
            .knowledge
            .get(actor_seat)
            .is_some_and(|knowledge| knowledge.located.contains(id))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Target evaluation uses the play's frozen authority view."
)]
impl Game {
    pub(super) fn internal_target(
        &self,
        execution: &mut Execution,
        decision: &Decision,
    ) -> Result<()> {
        let (wait, domain) = self.target_domain(execution, decision)?;
        if wait.bounds.minimum.0 > wait.capacity()? {
            return Err(crate::EngineFailure::IllegalPlay(
                "no legal play targets".into(),
            ));
        }
        execution.waiting = Some((
            DecisionView {
                token: DecisionToken {
                    execution: execution.id,
                    decision: decision.id,
                    epoch: 0,
                },
                actor: execution.context.controller,
                domain: InputDomain::Targets(domain),
            },
            Wait::Targets(wait),
        ));
        Ok(())
    }

    pub(in crate::game::internal) fn internal_resolution_target(
        &mut self,
        execution: &mut Execution,
        decision: &Decision,
    ) -> Result<()> {
        check_resolution(decision)?;
        let (mut wait, mut domain) = self.target_domain(execution, decision)?;
        let available = wait.capacity()?;
        // CR 1.3.2: a resolving instruction does as much as possible; play stays strict.
        wait.bounds.minimum = Nat(wait.bounds.minimum.0.min(available));
        domain.cardinality = wait.bounds;
        let actor = execution.player(self, &decision.actor)?;
        // Even an empty or singleton resolution domain remains a player's decision.
        self.internal_wait(
            execution,
            decision.id,
            actor,
            InputDomain::Targets(domain),
            Wait::ResolutionTargets(wait),
        )
    }

    fn target_domain(
        &self,
        execution: &Execution,
        decision: &Decision,
    ) -> Result<(TargetWait, TargetDomain)> {
        let ChoiceDomain::Objects {
            constraint,
            binding,
            ..
        } = &decision.domain
        else {
            return Err(unsupported("target domain"));
        };
        if constraint.groups.is_empty() || constraint.groups.len() > 2 {
            return Err(unsupported("target groups"));
        }
        let evolution = effect_evolution::is_choice(decision);
        let allow_resolution_zones = matches!(decision.kind, DecisionKind::ResolutionChoice { .. });
        let observed_deck = allow_resolution_zones && private_look_decision(decision, constraint);
        let actor = execution.player(self, &decision.actor)?;
        let actor_seat = super::seat_name(actor)?;
        let group_members = self.target_group_members(execution, decision, constraint)?;
        let (members, group_bounds, bounds) = group_layout(constraint, &group_members);
        let bounds = grouped_look_field_bounds(
            self,
            actor_seat,
            observed_deck,
            constraint.groups.len(),
            bounds,
        )?;
        let public = members
            .iter()
            .enumerate()
            .map(|(index, reference)| {
                let id = execution.object_id(*reference)?;
                let object = self.object(id)?;
                let kind = self.object_type(id)?;
                let private_observation =
                    private_observed_deck(self, id, actor_seat, observed_deck);
                if !(private_observation
                    || object.zone == "field"
                    || (evolution && object.zone == "evolve_deck")
                    || (allow_resolution_zones
                        && matches!(object.zone.as_str(), "ex" | "cemetery")))
                    || object.generation != reference.generation.0
                    || (object.zone == "field"
                        && !(kind.contains("フォロワー") || kind.contains("アミュレット")))
                {
                    return Err(unsupported(
                        "target is not a current public field object or EX card or cemetery card",
                    ));
                }
                if !evolution
                    && matches!(object.zone.as_str(), "field" | "ex")
                    && self.restricted(id, "choose")?
                {
                    return Err(unsupported("target permission"));
                }
                Ok(super::super::DistributionMember {
                    handle: u64::try_from(index).map_err(invalid)?,
                    player: player_id(&object.controller)?,
                    card: Some(object.card.clone()),
                    name: Some(self.card_name(id)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let characteristics = members
            .iter()
            .map(|reference| {
                Ok((
                    *reference,
                    characteristics(self, execution.object_id(*reference)?)?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let on_field_at_capture = characteristics
            .iter()
            .all(|(_, saved)| saved.zone == "field");
        Ok((
            TargetWait {
                decision: decision.clone(),
                binding: *binding,
                members,
                bounds,
                group_members,
                group_bounds,
                characteristics,
                distinct_names: constraint.distinct_by == Some(ObjectProperty::Names),
                on_field_at_capture,
            },
            TargetDomain {
                members: public,
                cardinality: bounds,
                distinct_names: constraint.distinct_by == Some(ObjectProperty::Names),
            },
        ))
    }

    fn target_group_members(
        &self,
        execution: &Execution,
        decision: &Decision,
        constraint: &SelectionConstraint,
    ) -> Result<Vec<Vec<ObjectRef>>> {
        if effect_evolution::is_choice(decision) {
            if constraint.groups.len() != 1 {
                return Err(unsupported("one effect evolution selector group"));
            }
            return Ok(vec![self.effect_evolution_members(execution)?]);
        }
        let allow_resolution_zones = matches!(decision.kind, DecisionKind::ResolutionChoice { .. });
        let observed_deck = allow_resolution_zones && private_look_decision(decision, constraint);
        let actor = execution.player(self, &decision.actor)?;
        let actor_seat = super::seat_name(actor)?;
        constraint
            .groups
            .iter()
            .map(|group| {
                self.target_domain_members(
                    execution,
                    &group.selector,
                    allow_resolution_zones,
                    observed_deck,
                    actor_seat,
                )
            })
            .collect()
    }

    fn target_domain_members(
        &self,
        execution: &Execution,
        selector: &Selector,
        allow_resolution_zones: bool,
        observed_deck: bool,
        actor_seat: &str,
    ) -> Result<Vec<ObjectRef>> {
        let mut members = Vec::new();
        for member in execution.public_target_objects(self, selector, allow_resolution_zones)? {
            let id = execution.object_id(member)?;
            let object = self.object(id)?;
            if private_observed_deck(self, id, actor_seat, observed_deck)
                || (object.zone != "deck"
                    && self.targetable_by(id, super::seat_name(execution.context.controller)?)?)
            {
                members.push(member);
            }
        }
        Ok(members)
    }

    pub(super) fn internal_mode_targets_feasible(
        &self,
        execution: &Execution,
        parameters: &[Decision],
    ) -> Result<bool> {
        for decision in parameters {
            let (wait, _) = self.target_domain(execution, decision)?;
            if wait.bounds.minimum.0 > wait.capacity()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub(super) fn internal_feasible_targets(
        &self,
        execution: &Execution,
        slots: &BTreeMap<(ScopeId, u64), Slot>,
        parameters: &VecDeque<Decision>,
    ) -> Result<bool> {
        if parameters.is_empty() {
            return Ok(true);
        }
        let mut candidate = execution.clone();
        candidate.slots.clone_from(slots);
        let first = parameters
            .front()
            .ok_or_else(|| invalid("missing target restart"))?;
        if let ChoiceDomain::Distribution {
            recipients, total, ..
        } = &first.domain
        {
            let recipients = candidate.recipients(self, recipients)?;
            let total = candidate.natural(self, total)?;
            if !recipients.members().is_empty()
                && u64::try_from(recipients.members().len()).map_err(invalid)? > total.0
            {
                return Ok(false);
            }
            candidate.captures(self, &first.captures)?;
            return self.internal_feasible_targets(
                &candidate,
                &candidate.slots,
                &parameters.iter().skip(1).cloned().collect(),
            );
        }
        if matches!(first.domain, ChoiceDomain::Recipients { .. }) {
            let (wait, _) = self.recipient_target_domain(&candidate, first)?;
            for members in wait.selections()? {
                let mut next = candidate.clone();
                next.slots.insert(
                    wait.binding.key(),
                    Slot::Recipients {
                        members,
                        characteristics: wait.characteristics.clone(),
                    },
                );
                if self.internal_feasible_targets(
                    &next,
                    &next.slots,
                    &parameters.iter().skip(1).cloned().collect(),
                )? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let (wait, _) = self.target_domain(&candidate, first)?;
        // Both public fields have a rules-defined capacity of five, so this proof is finite.
        if wait.members.len() > 10 {
            return Err(unsupported("target feasibility exceeds both fields"));
        }
        let end = 1_u64
            .checked_shl(u32::try_from(wait.members.len()).map_err(invalid)?)
            .ok_or_else(|| invalid("target plan overflow"))?;
        for mask in 0..end {
            let members = wait
                .members
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1_u64 << index) != 0)
                .map(|(_, member)| *member)
                .collect::<Vec<_>>();
            if !contains(
                wait.bounds,
                Nat(u64::try_from(members.len()).map_err(invalid)?),
            ) || !wait.compatible(&members)?
            {
                continue;
            }
            let mut candidate = candidate.clone();
            candidate.bind_targets(&wait, members);
            candidate.captures(self, &first.captures)?;
            if self.internal_feasible_targets(
                &candidate,
                &candidate.slots,
                &parameters.iter().skip(1).cloned().collect(),
            )? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Selected targets retain a checked producer separate from installation locals."
)]
impl Execution {
    pub(in crate::game::internal) fn is_private_look_target(
        &self,
        selector: &Selector,
        member: ObjectRef,
    ) -> bool {
        match selector {
            Selector::Bound { reference } => self
                .targets
                .get(&reference.key())
                .is_some_and(|wait| wait.is_private_look_member(member)),
            _ => false,
        }
    }

    pub(in crate::game::internal) fn public_field_objects(
        &self,
        game: &Game,
        selector: &Selector,
    ) -> Result<Vec<ObjectRef>> {
        self.public_target_objects(game, selector, false)
    }

    pub(in crate::game::internal) fn public_zone_objects(
        &self,
        game: &Game,
        selector: &Selector,
    ) -> Result<Vec<ObjectRef>> {
        self.public_target_objects(game, selector, true)
    }

    fn public_target_objects(
        &self,
        game: &Game,
        selector: &Selector,
        allow_resolution_zones: bool,
    ) -> Result<Vec<ObjectRef>> {
        let members = match selector {
            Selector::Zone {
                players,
                binding,
                predicate,
                zone,
                ..
            } => {
                let zone = match zone {
                    ZoneKind::Field => "field",
                    ZoneKind::Ex if allow_resolution_zones => "ex",
                    ZoneKind::Cemetery if allow_resolution_zones => "cemetery",
                    _ => return Err(unsupported("public target zone")),
                };
                check_predicate(predicate, *binding)?;
                let mut members = Vec::new();
                for player in self.players(game, players)? {
                    for id in game.zone_ids(super::super::seat_name(player)?, zone) {
                        if matches_candidate(game, &id, predicate)? {
                            members.push(self.reference(game, &id)?);
                        }
                    }
                }
                members
            }
            Selector::Union { sets } => sets
                .iter()
                .map(|set| self.public_target_objects(game, set, allow_resolution_zones))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect(),
            Selector::Filter {
                input,
                binding,
                predicate,
            } => {
                check_predicate(predicate, *binding)?;
                let mut filtered = Vec::new();
                for member in self.public_target_objects(game, input, allow_resolution_zones)? {
                    if matches_candidate(game, self.object_id(member)?, predicate)? {
                        filtered.push(member);
                    }
                }
                filtered
            }
            Selector::ReceiptMembers {
                receipt: ObjectReceipt::Look { .. },
                ..
            } => {
                if !allow_resolution_zones {
                    return Err(unsupported("look choice outside resolution"));
                }
                self.observed_objects(game, selector, self.context.controller)?
            }
            _ => self.objects(game, selector)?,
        };
        let mut seen = BTreeSet::new();
        Ok(members
            .into_iter()
            .filter(|member| seen.insert((member.id, member.generation)))
            .collect())
    }

    pub(in crate::game::internal) fn bind_targets(
        &mut self,
        wait: &TargetWait,
        members: Vec<ObjectRef>,
    ) {
        self.targets.insert(wait.binding.key(), wait.clone());
        self.slots.insert(
            wait.binding.key(),
            Slot::Objects {
                members,
                on_field_at_capture: wait.on_field_at_capture,
            },
        );
    }
}

fn look_filtered_selector(selector: &Selector) -> bool {
    matches!(selector,
        Selector::Filter { input, .. }
            if matches!(input.as_ref(), Selector::ReceiptMembers {
                receipt: ObjectReceipt::Look { .. },
                projection: MemberProjection::Before,
            })
    )
}
