use super::super::{DecisionToken, DecisionView, InputDomain, Slot, Wait, contains};
use super::{Execution, Game, Nat, ObjectRef, Result, invalid, player_id, unsupported};
use crate::game::Object;
use crate::game::internal::player::CandidateLinks;
use crate::game::player::Viewer;
use crate::game::player::{self, SelectionQuestion};
use crate::ir::events::timing::opponent;
use crate::ir::history_turn::public::damaged_plan;
use crate::ir::observation::{self, Consumer};
use crate::ir::observation_selection::{
    CardTotals, card_totals, held_by, private_hand_choice, private_look_choice, revealed_choice,
};
use crate::ir::{
    Action, Binding, Cardinality, ChoiceDomain, Comparison, Decision, DecisionKind, Disjointness,
    MoveDestination, MoveSpec, Node, ObjectOperand, ObjectProperty, ObjectReceipt, PlaySlot,
    PlayerRef, PlayerRelation, ScopeId, SelectionConstraint, SelectionGroup, Selector,
};
use crate::ir::{
    AggregateBound, NumericProperty, RoleCardinality, TargetRoleAnswer, TargetRoleSpec,
};
use crate::ir::{Bool, Expr, MemberProjection, SelectionView, ZoneKind};
use crate::ir::{Position, hand_position};
use crate::ir::{advance_entry, effect_evolution};
use crate::ir::{follow_keyword, public_evolve, public_numbers};
use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use serde_json::{Value, json};

pub(in crate::game::internal) mod constraints;
pub(in crate::game::internal) mod groups;

use groups::GroupConstraints;

use constraints::{
    DistinctProperties, PropertyKey, check_count, unique_handles, within_original_cost_cap,
};

/// Object candidates visible only to the current chooser, addressed within that question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetDomain {
    /// Token-scoped objects; a discard may show the chooser's own hand cards.
    pub members: Vec<super::super::DistributionMember>,
    /// Bounds for this question; play-time targets retain their strict printed minimum.
    pub cardinality: Cardinality,
    /// Selected objects must not share a rules name.
    pub distinct_names: bool,
    /// Printed costs keyed by handle when a cost-material answer requires distinct costs.
    pub original_costs: Option<BTreeMap<u64, i64>>,
    /// Roles assigned in the same answer as this target set.
    pub roles: Vec<TargetRoleDomain>,
}

/// Public role identity and cardinality, without authority-side bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetRoleDomain {
    /// Deck edge answered with a private card; absent for ordinary target roles.
    pub position: Option<Position>,
    /// Question-scoped role identifier.
    pub role: Nat,
    /// Printed rule for how many selected objects receive this role.
    pub cardinality: RoleCardinality,
}

/// One part of the disjoint target partition in a player's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRoleAssignment {
    /// Identifier from the question's role list.
    pub role: Nat,
    /// Handles from the selected set in the same question.
    pub targets: Vec<u64>,
}

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct TargetWait {
    links: CandidateLinks,
    pub(in crate::game::internal) decision: Decision,
    binding: Binding<Selector>,
    members: Vec<ObjectRef>,
    bounds: Cardinality,
    groups: GroupConstraints,
    characteristics: Vec<(ObjectRef, TargetCharacteristics)>,
    distinct_names: bool,
    cost_cap: Option<CostCap>,
    on_field_at_capture: bool,
}

/// A printed total original cost cap with each candidate's original cost.
#[derive(Clone, Debug)]
struct CostCap {
    limit: i64,
    costs: Vec<(ObjectRef, i64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::game::internal) struct TargetCharacteristics {
    controller: String,
    zone: String,
    kind: String,
    pub(in crate::game::internal) traits: Vec<String>,
    names: Vec<String>,
}

impl TargetCharacteristics {
    pub(in crate::game::internal) fn player_names(&self) -> Vec<String> {
        self.names.clone()
    }
}

pub(in crate::game::internal) fn characteristics(
    game: &Game,
    id: &str,
) -> Result<TargetCharacteristics> {
    let traits = game.card_traits(id)?.into_iter().collect();
    Ok(TargetCharacteristics {
        names: game.card_names(id)?,
        controller: game.object(id)?.controller.clone(),
        zone: game.object(id)?.zone.clone(),
        kind: game.object_type(id)?.into(),
        traits,
    })
}

impl TargetWait {
    pub(in crate::game::internal) fn player_question(
        &self,
        game: &Game,
        actor: Viewer,
        domain: &TargetDomain,
    ) -> Result<SelectionQuestion> {
        let references = self.members.iter().copied().map(Some).collect::<Vec<_>>();
        let mut question = game.player_selection(actor, domain, &references, &self.links)?;
        question.groups = self
            .groups
            .members
            .iter()
            .zip(&self.groups.bounds)
            .map(|(members, bounds)| {
                Ok(player::SelectionGroup {
                    handles: members
                        .iter()
                        .map(|member| {
                            let index = self
                                .members
                                .iter()
                                .position(|candidate| candidate == member)
                                .ok_or_else(|| invalid("missing selection group member"))?;
                            Ok(domain
                                .members
                                .get(index)
                                .ok_or_else(|| invalid("missing selection coordinate"))?
                                .handle)
                        })
                        .collect::<Result<_>>()?,
                    cardinality: if self.groups.members.len() == 1 {
                        self.bounds
                    } else {
                        *bounds
                    },
                })
            })
            .collect::<Result<_>>()?;
        for (member, reference) in question.members.iter_mut().zip(&self.members) {
            member.rules_names = Some(
                self.characteristics
                    .iter()
                    .find(|(candidate, _)| candidate == reference)
                    .ok_or_else(|| invalid("missing captured rules names"))?
                    .1
                    .names
                    .clone(),
            );
        }
        if let Some(cap) = &self.cost_cap {
            question.maximum_original_cost = Some(cap.limit);
            let costs = self
                .members
                .iter()
                .zip(&domain.members)
                .map(|(reference, member)| {
                    Ok((
                        member.handle,
                        cap.costs
                            .iter()
                            .find(|(candidate, _)| candidate == reference)
                            .ok_or_else(|| invalid("missing captured original cost"))?
                            .1,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            question.set_original_costs(costs);
        }
        Ok(question)
    }

    pub(in crate::game::internal) fn check_advance_current(
        &self,
        game: &Game,
        execution: &Execution,
        members: &[ObjectRef],
    ) -> Result<()> {
        let Some(name) = advance_entry::selection(&self.decision) else {
            return Ok(());
        };
        let seat = super::seat_name(execution.context.controller)?;
        for member in members {
            let id = execution.object_id(*member)?;
            if game.object(id)?.generation != member.generation.0
                || !game.named_advance_candidate(id, seat, &name)?
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "named Advance resource changed".into(),
                ));
            }
        }
        Ok(())
    }

    fn is_private_look_member(&self, member: ObjectRef) -> bool {
        self.members.contains(&member) && private_look_choice(&self.decision)
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
        if self.roles().is_some() {
            return Err(invalid("target roles required"));
        }
        self.validate_selection(handles)
    }

    const fn roles(&self) -> Option<&TargetRoleSpec> {
        match &self.decision.domain {
            ChoiceDomain::Objects { target_roles, .. } => target_roles.as_ref(),
            _ => None,
        }
    }

    pub(in crate::game::internal) fn validate_roles(
        &self,
        handles: &[u64],
        assignments: &[TargetRoleAssignment],
    ) -> Result<(Vec<ObjectRef>, Vec<TargetRoleAnswer>)> {
        let selected = self.validate_selection(handles)?;
        let spec = self
            .roles()
            .ok_or_else(|| invalid("unexpected target roles"))?;
        let answers = assignments
            .iter()
            .map(|assignment| {
                let objects = assignment
                    .targets
                    .iter()
                    .map(|handle| {
                        self.members
                            .get(usize::try_from(*handle).map_err(invalid)?)
                            .copied()
                            .ok_or_else(|| invalid("foreign role target handle"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(TargetRoleAnswer {
                    role: assignment.role,
                    objects,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if !spec.accepts_answer(&selected, &answers) {
            return Err(invalid("target role partition"));
        }
        Ok((selected, answers))
    }

    fn validate_selection(&self, handles: &[u64]) -> Result<Vec<ObjectRef>> {
        check_count(self.bounds, handles.len(), "target cardinality")?;
        let members = unique_handles(handles, "duplicate target handle", |handle| {
            self.members
                .get(usize::try_from(handle).map_err(invalid)?)
                .copied()
                .ok_or_else(|| invalid("foreign target handle"))
        })?;
        if !self.compatible(&members)? {
            return Err(invalid("target selection constraints"));
        }
        Ok(members)
    }

    fn compatible(&self, members: &[ObjectRef]) -> Result<bool> {
        if let Some(CostCap { limit, costs }) = &self.cost_cap
            && !within_original_cost_cap(
                members.iter().map(|member| {
                    costs
                        .iter()
                        .find(|(reference, _)| reference == member)
                        .map(|(_, cost)| *cost)
                        .ok_or_else(|| invalid("missing target original cost"))
                }),
                *limit,
                "target cost sum overflow",
            )?
        {
            return Ok(false);
        }
        if self.distinct_names {
            let mut names = DistinctProperties::default();
            for member in members {
                let saved = self
                    .characteristics
                    .iter()
                    .find(|(reference, _)| reference == member)
                    .ok_or_else(|| invalid("missing target names"))?;
                if !names.insert_member(saved.1.names.iter().map(|name| PropertyKey::Name(name))) {
                    return Ok(false);
                }
            }
        }
        // A single group is already bounded by `TargetWait::bounds`, whose minimum may be
        // lowered when a resolution has fewer available candidates than requested.
        if self.groups.members.len() == 1 {
            return Ok(true);
        }
        Ok(self.groups.accepts(members))
    }

    fn capacity(&self) -> Result<u64> {
        if self.groups.members.len() > 1 && !self.distinct_names && self.cost_cap.is_none() {
            return self.groups.capacity();
        }
        if !self.distinct_names && self.groups.members.len() <= 1 {
            let Some(CostCap { limit, costs }) = &self.cost_cap else {
                return u64::try_from(self.members.len()).map_err(invalid);
            };
            // Cheapest cards first maximize how many fit under one total cap.
            let mut ascending = costs.iter().map(|(_, cost)| *cost).collect::<Vec<_>>();
            ascending.sort_unstable();
            let mut total = 0_i64;
            let mut count = 0_u64;
            for cost in ascending {
                total = total
                    .checked_add(cost)
                    .ok_or_else(|| invalid("target cost sum overflow"))?;
                if total > *limit {
                    break;
                }
                count = count.saturating_add(1);
            }
            return Ok(count);
        }
        if self.groups.members.len() == 1
            && let Some(maximum) = self.bounds.maximum.filter(|maximum| maximum.0 <= 2)
        {
            return self.pair_capacity(maximum.0);
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

    fn pair_capacity(&self, maximum: u64) -> Result<u64> {
        if maximum == 0 {
            return Ok(0);
        }
        // Availability above the requested ceiling is irrelevant; cemetery pools exceed fields.
        let mut available = 0;
        for (index, left) in self.members.iter().enumerate() {
            if self.compatible(&[*left])? {
                available = 1;
                if maximum == 1 {
                    return Ok(available);
                }
            }
            if maximum == 2 {
                for right in self.members.iter().skip(index.saturating_add(1)) {
                    if self.compatible(&[*left, *right])? {
                        return Ok(2);
                    }
                }
            }
        }
        Ok(available)
    }
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

/// A multi-card pick that enters the field next may only offer the free field slots;
/// a single card that cannot enter simply stays with the remainder.
fn field_entry_bounds(
    game: &Game,
    destination_seat: &str,
    feeds_field: bool,
    mut bounds: Cardinality,
) -> Result<Cardinality> {
    if feeds_field && bounds.maximum.is_none_or(|maximum| maximum.0 > 1) {
        let free_field_slots = u64::try_from(
            5_i64
                .saturating_sub(game.zone_count(destination_seat, "field"))
                .max(0),
        )
        .map_err(invalid)?;
        let maximum = bounds
            .maximum
            .map_or(free_field_slots, |maximum| maximum.0.min(free_field_slots));
        bounds.maximum = Some(Nat(maximum));
        bounds.minimum = Nat(bounds.minimum.0.min(maximum));
    }
    Ok(bounds)
}

fn target_group(constraint: &SelectionConstraint) -> Result<&SelectionGroup> {
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("one selector group per target decision"));
    };
    if !matches!(constraint.distinct_by, None | Some(ObjectProperty::Names))
        || !(count_bounds(constraint)
            || matches!(
                card_totals(&constraint.aggregate_bounds),
                CardTotals::OriginalCostAtMost(_)
            ))
        || constraint.disjoint != Disjointness::Disjoint
    {
        return Err(unsupported("target properties or aggregate constraints"));
    }
    Ok(group)
}

fn count_bounds(constraint: &SelectionConstraint) -> bool {
    if constraint.aggregate_bounds.is_empty() {
        return true;
    }
    let [
        AggregateBound {
            property: NumericProperty::Count,
            comparison,
            value,
        },
    ] = constraint.aggregate_bounds.as_slice()
    else {
        return false;
    };
    constraint.groups.len() == 1
        && constraint.groups[0].cardinality == constraint.cardinality
        && constraint.cardinality.minimum == Nat(0)
        && constraint.cardinality.maximum.is_none()
        && matches!(comparison, Comparison::Equal | Comparison::LessEqual)
        && public_numbers::target_count(value, *comparison)
}
fn evaluated_counts(
    game: &Game,
    execution: &Execution,
    printed: &SelectionConstraint,
    decision: &Decision,
) -> Result<SelectionConstraint> {
    if matches!(
        card_totals(&printed.aggregate_bounds),
        CardTotals::OriginalCostAtMost(_)
    ) {
        return Ok(printed.clone());
    }
    let formal_targets = matches!(
        decision.kind,
        DecisionKind::PlayParameter {
            slot: PlaySlot::Targets,
            ..
        }
    );
    if !count_bounds(printed)
        || (!formal_targets
            && printed
                .aggregate_bounds
                .iter()
                .any(|bound| !public_numbers::check(&bound.value)))
    {
        return Err(unsupported("dynamic target count shape"));
    }
    let mut constraint = printed.clone();
    if let [bound] = constraint.aggregate_bounds.as_slice() {
        let count =
            Nat(u64::try_from(execution.integer(game, &bound.value)?.0.max(0)).map_err(invalid)?);
        let cardinality = Cardinality {
            minimum: if bound.comparison == Comparison::Equal {
                count
            } else {
                Nat(0)
            },
            maximum: Some(count),
        };
        constraint.cardinality = cardinality;
        constraint.groups[0].cardinality = cardinality;
        constraint.aggregate_bounds.clear();
    }
    Ok(constraint)
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
    if constraint.groups.len() > 1 {
        let count = u64::try_from(constraint.groups.len()).map_err(invalid)?;
        let required = constraint.cardinality.minimum != Nat(0);
        if !matches!(count, 2 | 3)
            || constraint.cardinality.maximum != Some(Nat(count))
            || (required && (count != 2 || constraint.cardinality.minimum != Nat(count)))
            || constraint.distinct_by.is_some()
            || !constraint.aggregate_bounds.is_empty()
            || constraint.disjoint != Disjointness::Disjoint
        {
            return Err(unsupported("formal target group bounds"));
        }
        for group in &constraint.groups {
            if group.cardinality
                != (Cardinality {
                    minimum: Nat(u64::from(required)),
                    maximum: Some(Nat(1)),
                })
                || !matches!(&group.selector, Selector::Zone { zone: ZoneKind::Cemetery | ZoneKind::Banish, players, .. }
                if players.relation == PlayerRelation::SelfPlayer && players.relative_to == (PlayerRef::Controller {}))
            {
                return Err(unsupported("formal target group selector"));
            }
            target_selector(&group.selector)?;
        }
        return Ok(());
    }
    target_selector(&target_group(constraint)?.selector)
}

fn public_object(object: &Object) -> bool {
    matches!(object.zone.as_str(), "field" | "ex" | "cemetery")
        || (object.zone == "banish" && object.state["face_up"] != false)
}

fn candidate_zone(selector: &Selector, allow_resolution_zones: bool) -> Result<&'static str> {
    let Selector::Zone { players, zone, .. } = selector else {
        return Err(unsupported("public target zone"));
    };
    Ok(match zone {
        ZoneKind::Field => "field",
        ZoneKind::Ex => "ex",
        ZoneKind::Evolve
            if (allow_resolution_zones || public_evolve::selector(selector))
                && players.relation == PlayerRelation::SelfPlayer =>
        {
            "evolve_deck"
        }
        // Loading already proved the chooser holds this hand; either player may be that holder.
        ZoneKind::Hand
            if allow_resolution_zones
                && (held_by(players, &PlayerRef::Controller {})
                    || held_by(players, &opponent())) =>
        {
            "hand"
        }
        ZoneKind::Cemetery => "cemetery",
        ZoneKind::Banish
            if players.relation == PlayerRelation::SelfPlayer
                && players.relative_to == (PlayerRef::Controller {}) =>
        {
            "banish"
        }
        _ => return Err(unsupported("public target zone")),
    })
}

fn target_selector(selector: &Selector) -> Result<()> {
    match selector {
        Selector::Union { sets }
            if sets.len() == 2 && sets.iter().all(|set| matches!(set, Selector::Zone { .. })) =>
        {
            target_union(sets)
        }
        Selector::Zone {
            zone: ZoneKind::Evolve,
            ..
        } if public_evolve::selector(selector) => public_zone_selector(selector),
        Selector::Zone {
            zone: ZoneKind::Ex, ..
        } => public_zone_selector(selector),
        Selector::Zone {
            zone: ZoneKind::Cemetery,
            view: SelectionView::Current {},
            players,
            binding,
            predicate,
        } if players.relative_to == (PlayerRef::Controller {})
            && matches!(
                players.relation,
                PlayerRelation::SelfPlayer | PlayerRelation::Opponents
            ) =>
        {
            check_predicate(predicate, *binding, false)
        }
        Selector::Zone {
            zone: ZoneKind::Banish,
            view: SelectionView::Current {},
            players,
            binding,
            predicate,
        } if players.relative_to == (PlayerRef::Controller {})
            && players.relation == PlayerRelation::SelfPlayer =>
        {
            check_predicate(predicate, *binding, false)
        }
        Selector::Difference { input, except } if matches!(except.as_ref(), Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}]) => {
            target_selector(input)
        }
        selector => field_selector(selector),
    }
}

fn target_union(sets: &[Selector]) -> Result<()> {
    let field_ex = sets.iter().all(|set| {
        matches!(
            set,
            Selector::Zone {
                zone: ZoneKind::Field | ZoneKind::Ex,
                ..
            }
        )
    });
    let public_return = matches!(
        sets,
        [
            Selector::Zone {
                zone: ZoneKind::Cemetery,
                ..
            },
            Selector::Zone {
                zone: ZoneKind::Banish,
                ..
            }
        ] | [
            Selector::Zone {
                zone: ZoneKind::Banish,
                ..
            },
            Selector::Zone {
                zone: ZoneKind::Cemetery,
                ..
            }
        ]
    ) && sets.iter().all(own_zone_selector);
    if !field_ex && !public_return {
        return Err(unsupported("public target union zones"));
    }
    for set in sets {
        if public_return {
            target_selector(set)?;
        } else {
            public_zone_selector(set)?;
        }
    }
    Ok(())
}

pub(in crate::game::internal) fn check_resolution(decision: &Decision) -> Result<()> {
    if effect_evolution::is_choice(decision) || advance_entry::selection(decision).is_some() {
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
        target_roles,
        ..
    } = &decision.domain
    else {
        return Err(unsupported("resolution public choice domain"));
    };
    if target_roles.is_some() && hand_position::choice(decision).is_none() {
        return Err(unsupported("resolution position roles"));
    }
    if private_hand_choice(decision) {
        let [group] = constraint.groups.as_slice() else {
            return Err(unsupported("one hand group"));
        };
        let branches: Vec<&Selector> = if let Selector::Union { sets } = &group.selector {
            sets.iter().collect()
        } else {
            vec![&group.selector]
        };
        for branch in branches {
            let Selector::Zone {
                binding, predicate, ..
            } = branch
            else {
                return Err(unsupported("hand selector"));
            };
            super::super::search::check_predicate(predicate, *binding)?;
        }
        return Ok(());
    }
    if revealed_groups(constraint) {
        if !super::super::alternatives::participant(&decision.actor) || !revealed_choice(decision) {
            return Err(unsupported("revealed choice actor or bounds"));
        }
        for group in &constraint.groups {
            let (binding, predicate) = revealed_filter(&group.selector)
                .ok_or_else(|| unsupported("revealed group selector"))?;
            // Like a looked card, a revealed one may be compared with the effect's source.
            check_predicate(predicate, binding, true)?;
        }
        return Ok(());
    }
    let private_look_groups = !constraint.groups.is_empty()
        && constraint
            .groups
            .iter()
            .all(|group| private_look_filter(&group.selector).is_some());
    if private_look_groups {
        if !private_look_choice(decision) {
            return Err(unsupported("private observed choice actor or bounds"));
        }
        for group in &constraint.groups {
            let (binding, predicate) = private_look_filter(&group.selector)
                .ok_or_else(|| unsupported("private observed group selector"))?;
            check_predicate(predicate, binding, true)?;
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
            check_predicate(predicate, *binding, false)?;
        }
        selector => target_selector(selector)?,
    }
    if !super::super::alternatives::participant(&decision.actor)
        || !matches!(
            selection_zone(&group.selector),
            Some(ZoneKind::Field | ZoneKind::Ex | ZoneKind::Cemetery | ZoneKind::Evolve)
        )
    {
        return Err(unsupported("resolution public choice actor or selector"));
    }
    Ok(())
}

/// An own-EX resolution pick (BP13-010) is public but enters the field like a private one.
fn own_ex_pick(constraint: &SelectionConstraint) -> bool {
    matches!(constraint.groups.as_slice(), [group]
        if selection_zone(&group.selector) == Some(ZoneKind::Ex)
            && own_zone_selector(&group.selector))
}

fn own_zone_selector(selector: &Selector) -> bool {
    match selector {
        Selector::Zone { players, .. } => {
            players.relation == PlayerRelation::SelfPlayer
                && players.relative_to == (PlayerRef::Controller {})
        }
        Selector::Difference { input, .. } | Selector::Extremum { input, .. } => {
            own_zone_selector(input)
        }
        _ => false,
    }
}

fn selection_zone(selector: &Selector) -> Option<ZoneKind> {
    match selector {
        Selector::Zone { zone, .. } => Some(*zone),
        Selector::Difference { input, .. } | Selector::Extremum { input, .. } => {
            selection_zone(input)
        }
        _ => None,
    }
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
            check_predicate(predicate, *binding, false)
        }
        // "それ以外" removes an earlier target binding; "他の" removes the source holder.
        Selector::Difference { input, except }
            if matches!(except.as_ref(), Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}])
                || matches!(except.as_ref(), Selector::Bound { .. }) =>
        {
            field_selector(input)
        }
        Selector::Extremum {
            input, property, ..
        } => {
            super::super::aggregates::check(input, *property)?;
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
    if follow_keyword::supported(selector) {
        return Ok(());
    }
    match selector {
        Selector::Difference { input, except } if matches!(except.as_ref(), Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}]) => {
            public_zone_selector(input)
        }
        Selector::Extremum {
            input, property, ..
        } => {
            super::super::aggregates::check(input, *property)?;
            public_zone_selector(input)
        }
        Selector::Filter {
            input,
            binding,
            predicate,
        } if matches!(input.as_ref(), Selector::Bound { .. }) => {
            check_predicate(predicate, *binding, false)
        }
        Selector::Zone {
            zone: zone @ (ZoneKind::Field | ZoneKind::Ex | ZoneKind::Cemetery | ZoneKind::Evolve),
            view: SelectionView::Current {},
            predicate,
            binding,
            players,
        } if players.relative_to == (PlayerRef::Controller {})
            && (*zone != ZoneKind::Evolve || players.relation == PlayerRelation::SelfPlayer) =>
        {
            check_predicate(predicate, *binding, false)
        }
        _ => Err(unsupported("public count selector")),
    }
}

fn revealed_groups(constraint: &SelectionConstraint) -> bool {
    !constraint.groups.is_empty()
        && constraint
            .groups
            .iter()
            .all(|group| revealed_filter(&group.selector).is_some())
}

/// A choice among members of one public reveal, still in the zone where they were shown.
fn revealed_filter(selector: &Selector) -> Option<(Binding<ObjectRef>, &Expr<Bool>)> {
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
            receipt: ObjectReceipt::Reveal { .. },
            projection: MemberProjection::Before,
        }
    )
    .then_some((*binding, predicate))
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
    observation::classify(input, Consumer::Choice, observation::Position::Input)
        .is_some()
        .then_some((*binding, predicate))
}

pub(in crate::game::internal) fn check_predicate(
    value: &Expr<Bool>,
    candidate: Binding<ObjectRef>,
    private_observation: bool,
) -> Result<()> {
    let policy = if private_observation {
        super::super::conditions::Policy::PrivateSelection
    } else {
        super::super::conditions::Policy::Selection
    };
    super::super::conditions::ConditionPlan::compile(value, candidate, policy).map(|_checked| ())
}

pub(in crate::game::internal) fn matches_candidate(
    game: &Game,
    id: &str,
    condition: &super::super::conditions::PreparedCondition,
) -> Result<bool> {
    super::super::conditions::candidate::matches(condition, game, id)
}

fn role_domains(decision: &Decision) -> Vec<TargetRoleDomain> {
    let positioned = hand_position::choice(decision).is_some();
    match &decision.domain {
        ChoiceDomain::Objects {
            target_roles: Some(spec),
            ..
        } => spec
            .roles
            .iter()
            .map(|role| TargetRoleDomain {
                position: positioned.then_some(if role.role == Nat(1) {
                    Position::Top {}
                } else {
                    Position::Bottom {}
                }),
                role: role.role,
                cardinality: role.cardinality,
            })
            .collect(),
        _ => vec![],
    }
}

pub(in crate::game::internal) fn packet(domain: &TargetDomain) -> Value {
    let roles = domain.roles.iter().map(|role| {
        let cardinality = match role.cardinality {
            RoleCardinality::IfNonempty { count } => json!({"if_nonempty": count.0}),
            RoleCardinality::Remainder {} => json!({"remaining": true}),
            RoleCardinality::Fixed { cardinality } => json!({"minimum": cardinality.minimum.0, "maximum": cardinality.maximum.map(|n| n.0)}),
        };
        let mut entry = json!({"role": role.role.0, "cardinality": cardinality});
        if let Some(position) = role.position {
            entry["position"] = match position {
                Position::Top {} => json!("top"),
                Position::Bottom {} => json!("bottom"),
                Position::Index { index } => json!({"nth":index.0.get()}),
            };
        }
        entry
    }).collect::<Vec<_>>();
    let members = domain.members.iter().map(|member| {
        let mut candidate = json!({"handle":member.handle,"player":member.player.seat,"card":member.card,"name":member.name});
        if let Some(cost) = domain.original_costs.as_ref().and_then(|costs| costs.get(&member.handle)) {
            candidate["orig_cost"] = json!(cost);
        }
        candidate
    }).collect::<Vec<_>>();
    let mut packet = json!({"kind":"targets","roles":roles,"distinct_names":domain.distinct_names,"minimum":domain.cardinality.minimum.0,"maximum":domain.cardinality.maximum.map(|n| n.0),"members":members});
    if domain.original_costs.is_some() {
        packet["distinct_by"] = json!("orig_cost");
    }
    packet
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Target evaluation uses the play's frozen authority view."
)]
impl Game {
    pub(in crate::game::internal) fn current_observed_deck(
        &self,
        execution: &Execution,
        member: ObjectRef,
        actor: &str,
    ) -> Result<bool> {
        let id = execution.object_id(member)?;
        let object = self.object(id)?;
        Ok(object.generation == member.generation.0
            && object.zone == "deck"
            && object.owner == actor
            && object.controller == actor
            && self
                .zone_ids(actor, "deck")
                .iter()
                .any(|current| current == id)
            && self
                .state
                .knowledge
                .get(actor)
                .is_some_and(|knowledge| knowledge.knows_location(id)))
    }

    pub(in crate::game::internal) fn private_look_deck_member(
        &self,
        execution: &Execution,
        selector: &Selector,
        member: ObjectRef,
    ) -> Result<bool> {
        Ok(execution.is_private_look_target(selector, member)
            && self.current_observed_deck(
                execution,
                member,
                super::seat_name(execution.context.controller)?,
            )?)
    }

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

    #[expect(
        clippy::too_many_lines,
        reason = "Target candidates, captured characteristics and links freeze together."
    )]
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
        let observed_deck = private_look_choice(decision);
        if constraint.groups.is_empty() || constraint.groups.len() > 3 {
            return Err(unsupported("target groups"));
        }
        let evolution = effect_evolution::is_choice(decision);
        let private_evolve = evolution || advance_entry::selection(decision).is_some();
        let allow_resolution_zones = matches!(decision.kind, DecisionKind::ResolutionChoice { .. });
        let own_hand = allow_resolution_zones && private_hand_choice(decision);
        let revealed = revealed_groups(constraint);
        let actor_seat = super::seat_name(execution.player(self, &decision.actor)?)?;
        let constraint = evaluated_counts(self, execution, constraint, decision)?;
        let group_members = self.target_group_members(execution, decision, &constraint)?;
        let (members, group_bounds, bounds) = group_layout(&constraint, &group_members);
        // The capacity is the receiving field's, which need not be the chooser's.
        let receiver = match execution.field_entry_receiver(binding.key()) {
            Some(player) => execution.player(self, player)?,
            None => execution.context.controller,
        };
        let bounds = field_entry_bounds(
            self,
            super::seat_name(receiver)?,
            (observed_deck || own_hand || (allow_resolution_zones && own_ex_pick(&constraint)))
                && execution.field_entry_receiver(binding.key()).is_some(),
            bounds,
        )?;
        let public = members
            .iter()
            .enumerate()
            .map(|(index, reference)| {
                let id = execution.object_id(*reference)?;
                let object = self.object(id)?;
                let kind = self.object_type(id)?;
                let private_observation = observed_deck
                    && self.current_observed_deck(execution, *reference, actor_seat)?;
                if !(private_observation
                    || (revealed && object.zone == "hand" && self.publicly_located(id))
                    || (own_hand
                        && object.zone == "hand"
                        && object.owner == actor_seat
                        && object.controller == actor_seat)
                    || public_object(object)
                    || ((private_evolve
                        || (object.controller == actor_seat && object.state["face_up"] == true))
                        && object.zone == "evolve_deck"))
                    || object.generation != reference.generation.0
                    || (object.zone == "field"
                        && !(kind.contains("フォロワー") || kind.contains("アミュレット")))
                {
                    return Err(unsupported(
                        "target is not a current public field, EX, cemetery or banish card",
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
                links: CandidateLinks::for_selectors(
                    self,
                    execution,
                    members.iter().copied(),
                    constraint.groups.iter().map(|group| &group.selector),
                )?,
                decision: decision.clone(),
                binding: *binding,
                cost_cap: self.target_cost_cap(execution, &constraint, &members)?,
                members,
                bounds,
                groups: GroupConstraints {
                    members: group_members,
                    bounds: group_bounds,
                },
                characteristics,
                distinct_names: constraint.distinct_by == Some(ObjectProperty::Names),
                on_field_at_capture,
            },
            TargetDomain {
                members: public,
                cardinality: bounds,
                distinct_names: constraint.distinct_by == Some(ObjectProperty::Names),
                original_costs: None,
                roles: role_domains(decision),
            },
        ))
    }

    /// Original costs are printed data, so they are read once when the question is asked.
    fn target_cost_cap(
        &self,
        execution: &Execution,
        constraint: &SelectionConstraint,
        members: &[ObjectRef],
    ) -> Result<Option<CostCap>> {
        let limit = match card_totals(&constraint.aggregate_bounds) {
            CardTotals::Unbounded => return Ok(None),
            CardTotals::OriginalCostAtMost(limit) => limit,
            CardTotals::Unsupported => return Err(unsupported("target aggregate bound")),
        };
        let costs = members
            .iter()
            .map(|reference| {
                let id = execution.object_id(*reference)?;
                Ok((
                    *reference,
                    super::super::cost::printed_cost(&self.internal_cost_print(id)?["cost"])?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(CostCap { limit, costs }))
    }

    fn target_group_members(
        &self,
        execution: &Execution,
        decision: &Decision,
        constraint: &SelectionConstraint,
    ) -> Result<Vec<Vec<ObjectRef>>> {
        if let Some(name) = advance_entry::selection(decision) {
            let seat = super::seat_name(execution.context.controller)?;
            let mut members = Vec::new();
            for id in self.zone_ids(seat, "evolve_deck") {
                if self.named_advance_candidate(&id, seat, &name)? {
                    members.push(execution.reference(self, &id)?);
                }
            }
            return Ok(vec![members]);
        }
        if effect_evolution::is_choice(decision) {
            if constraint.groups.len() != 1 {
                return Err(unsupported("one effect evolution selector group"));
            }
            let specification = effect_evolution::specification(decision)
                .ok_or_else(|| unsupported("effect evolution destination"))?;
            return Ok(vec![
                self.effect_evolution_members(execution, &specification)?,
            ]);
        }
        let allow_resolution_zones = matches!(decision.kind, DecisionKind::ResolutionChoice { .. });
        let observed_deck = allow_resolution_zones && private_look_choice(decision);
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
            if observed_deck {
                if self.current_observed_deck(execution, member, actor_seat)? {
                    members.push(member);
                }
                continue;
            }
            let id = execution.object_id(member)?;
            let object = self.object(id)?;
            if object.zone != "deck" && self.targetable_by(id, actor_seat)? {
                members.push(member);
            }
        }
        Ok(members)
    }

    pub(super) fn named_advance_candidate(
        &self,
        id: &str,
        seat: &str,
        selection: &advance_entry::AdvanceSelection,
    ) -> Result<bool> {
        let object = self.object(id)?;
        let printed = self.catalog.face(&object.card, 0)?;
        Ok(object.zone == "evolve_deck"
            && object.owner == seat
            && object.controller == seat
            && object.state["face"] == 0_i64
            && object.state["face_up"] != true
            && printed["card_type"]
                == match selection.kind {
                    advance_entry::AdvanceKind::Follower => "フォロワー・アドバンス",
                    advance_entry::AdvanceKind::Spell => "スペル・アドバンス",
                }
            && self.card_name_matches(id, |own| selection.name.matches(own))?)
    }

    pub(super) fn internal_mode_targets_feasible(
        &self,
        execution: &Execution,
        parameters: &[Decision],
    ) -> Result<bool> {
        for decision in parameters
            .iter()
            .filter(|decision| !super::material::is_parameter(decision))
        {
            let feasible = if matches!(decision.domain, ChoiceDomain::Recipients { .. }) {
                let (_, domain) = self.recipient_target_domain(execution, decision)?;
                domain.cardinality.minimum.0
                    <= u64::try_from(domain.members.len()).map_err(invalid)?
            } else {
                let (wait, _) = self.target_domain(execution, decision)?;
                wait.bounds.minimum.0 <= wait.capacity()?
            };
            if !feasible {
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
            if !super::super::allocation::feasible(&candidate, self, recipients, total)? {
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
        if parameters.len() == 1 {
            return Ok(wait.bounds.minimum.0 <= wait.capacity()?);
        }
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
    pub(in crate::game::internal) fn record_transferred_controller(
        &mut self,
        member: ObjectRef,
        before: &str,
        after: &str,
    ) {
        let targets = self
            .targets
            .values_mut()
            .flat_map(|wait| wait.characteristics.iter_mut());
        let recipients = self
            .slots
            .values_mut()
            .filter_map(|slot| {
                if let Slot::Recipients {
                    characteristics, ..
                } = slot
                {
                    Some(characteristics)
                } else {
                    None
                }
            })
            .flat_map(|characteristics| characteristics.iter_mut());
        // Only the successful producer's controller delta may advance a frozen target.
        for (candidate, saved) in targets.chain(recipients) {
            if *candidate == member && saved.controller == before {
                after.clone_into(&mut saved.controller);
            }
        }
    }

    /// Lowering admits a private multi-card pick onto the field only as the very next step.
    /// The player whose field the very next step fills with this selection, if any.
    fn field_entry_receiver(&self, key: (ScopeId, u64)) -> Option<&PlayerRef> {
        let Some(Node::Instruction { instruction }) = self.todo.front() else {
            return None;
        };
        match &instruction.action {
            Action::Move {
                objects: Selector::Bound { reference },
                movement:
                    movement @ MoveSpec {
                        to: MoveDestination::Shared { zone },
                        ..
                    },
                ..
            } if reference.key() == key && super::super::movement::field_move(movement) => {
                Some(&zone.player)
            }
            _ => None,
        }
    }

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

    pub(in crate::game::internal) fn is_private_hand_target(
        &self,
        selector: &Selector,
        member: ObjectRef,
    ) -> bool {
        match selector {
            Selector::Bound { reference } => {
                self.targets.get(&reference.key()).is_some_and(|wait| {
                    wait.members.contains(&member) && private_hand_choice(&wait.decision)
                })
            }
            _ => false,
        }
    }

    pub(in crate::game::internal) fn is_private_advance_target(
        &self,
        selector: &Selector,
        member: ObjectRef,
    ) -> bool {
        match selector {
            Selector::Bound { reference } => {
                self.targets.get(&reference.key()).is_some_and(|wait| {
                    wait.members.contains(&member)
                        && advance_entry::selection(&wait.decision).is_some()
                })
            }
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

    fn damaged_followers(&self, game: &Game) -> Result<Vec<ObjectRef>> {
        let mut members = Vec::new();
        let opponent = self.player(game, &opponent())?;
        for id in game.zone_ids(super::super::seat_name(opponent)?, "field") {
            let object = game.object(&id)?;
            let reference = self.reference(game, &id)?;
            if object.generation == reference.generation.0
                && object.state["damaged_this_turn"] == true
                && game.object_type(&id)?.contains("フォロワー")
            {
                members.push(reference);
            }
        }
        Ok(members)
    }

    /// A card matching several branches is still one candidate.
    fn union_target_objects(
        &self,
        game: &Game,
        sets: &[Selector],
        allow_resolution_zones: bool,
    ) -> Result<Vec<ObjectRef>> {
        let mut members = Vec::new();
        for set in sets {
            for member in self.public_target_objects(game, set, allow_resolution_zones)? {
                if !members.contains(&member) {
                    members.push(member);
                }
            }
        }
        Ok(members)
    }

    pub(in crate::game::internal) fn public_target_objects(
        &self,
        game: &Game,
        selector: &Selector,
        allow_resolution_zones: bool,
    ) -> Result<Vec<ObjectRef>> {
        if damaged_plan(selector).is_some() {
            return self.damaged_followers(game);
        }
        let members = match selector {
            Selector::Zone {
                players,
                binding,
                predicate,
                ..
            } => {
                let zone = candidate_zone(selector, allow_resolution_zones)?;
                // Public candidates may exclude the source's own names ("これと同名を除く").
                let predicate = super::super::conditions::ConditionPlan::compile(
                    predicate,
                    *binding,
                    super::super::conditions::Policy::Selection,
                )?
                .prepare(Some(
                    &super::super::conditions::candidate::Operands {
                        game,
                        execution: self,
                    },
                ))?;
                let mut members = Vec::new();
                for player in self.players(game, players)? {
                    for id in game.zone_ids(super::super::seat_name(player)?, zone) {
                        if zone == "banish" && game.object(&id)?.state["face_up"] == false {
                            continue;
                        }
                        if matches_candidate(game, &id, &predicate)? {
                            members.push(self.reference(game, &id)?);
                        }
                    }
                }
                members
            }
            Selector::Union { sets } => {
                self.union_target_objects(game, sets, allow_resolution_zones)?
            }
            Selector::Filter {
                input,
                binding,
                predicate,
            } => self.filtered_target_objects(
                game,
                selector,
                input,
                *binding,
                predicate,
                allow_resolution_zones,
            )?,
            Selector::ReceiptMembers {
                receipt: ObjectReceipt::Look { .. },
                ..
            } => {
                if !allow_resolution_zones {
                    return Err(unsupported("look choice outside resolution"));
                }
                self.observed_objects(game, selector, self.context.controller, Consumer::Choice)?
            }
            _ => self.objects(game, selector)?,
        };
        let mut seen = BTreeSet::new();
        Ok(members
            .into_iter()
            .filter(|member| seen.insert((member.id, member.generation)))
            .collect())
    }

    fn filtered_target_objects(
        &self,
        game: &Game,
        selector: &Selector,
        input: &Selector,
        binding: Binding<ObjectRef>,
        predicate: &Expr<Bool>,
        allow_resolution_zones: bool,
    ) -> Result<Vec<ObjectRef>> {
        let private_look = private_look_filter(selector).is_some();
        let revealed = revealed_filter(selector).is_some();
        let policy = if private_look || revealed {
            super::super::conditions::Policy::PrivateSelection
        } else {
            super::super::conditions::Policy::Selection
        };
        let plan = super::super::conditions::ConditionPlan::compile(predicate, binding, policy)?;
        let operands = super::super::conditions::candidate::Operands {
            game,
            execution: self,
        };
        let condition = plan.prepare((private_look || revealed).then_some(&operands))?;
        let mut filtered = Vec::new();
        for member in self.public_target_objects(game, input, allow_resolution_zones)? {
            if private_look
                && !game.current_observed_deck(
                    self,
                    member,
                    super::super::seat_name(self.context.controller)?,
                )?
            {
                continue;
            }
            if follow_keyword::supported(selector)
                && game.object(self.object_id(member)?)?.zone != "field"
            {
                continue;
            }
            if !private_look
                && game.object(self.object_id(member)?)?.generation != member.generation.0
            {
                continue;
            }
            let id = self.object_id(member)?;
            let zone = game.object(id)?.zone.as_str();
            let public = matches!(zone, "field" | "ex" | "cemetery")
                || (revealed && zone == "hand" && game.publicly_located(id));
            if !private_look && !public {
                return Err(unsupported("selected condition requires public objects"));
            }
            if matches_candidate(game, self.object_id(member)?, &condition)? {
                filtered.push(member);
            }
        }
        Ok(filtered)
    }

    pub(in crate::game::internal) fn bind_target_roles(
        &mut self,
        wait: &TargetWait,
        answers: &[TargetRoleAnswer],
    ) {
        if let Some(spec) = wait.roles() {
            for role in &spec.roles {
                if let Some(answer) = answers.iter().find(|answer| answer.role == role.role) {
                    self.targets.insert(role.binding.key(), wait.clone());
                    self.slots.insert(
                        role.binding.key(),
                        Slot::Objects {
                            members: answer.objects.clone(),
                            on_field_at_capture: wait.on_field_at_capture,
                        },
                    );
                }
            }
        }
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
