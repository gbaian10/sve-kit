//! Cost materials use their own strict selection, without target permissions.
use super::{Execution, Game, Result, invalid, player_id, seat_name, unsupported};
use crate::game::internal::{
    DecisionToken, DecisionView, DistributionMember, InputDomain, Slot, Wait, contains, search,
};
use crate::game::legal::subsets;
use crate::ir::quick::{card_permission, is_printed_quick};
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::{
    AbilityKind, Action, Binding, Cardinality, Characteristics, ChoiceDomain, CostSpec, Decision,
    DecisionKind, Disjointness, ExecutionRef, InputMode, Instruction, Int, MoveDestination,
    MoveMethod, MoveReason, MoveSpec, Nat, ObjectOperand, ObjectRef, OwnerZone, PlaySlot, PlaySpec,
    PlayerRef, PlayerRelation, Posture, PostureEvent, Quantity, QuantityUnit, SelectionSnapshot,
    SelectionView, Selector, Visibility, WellFormedCardProgram, ZoneKind, ZoneRef,
};
use alloc::collections::BTreeSet;

#[derive(Clone, Debug)]
pub(in crate::game::internal) struct MaterialWait {
    binding: Binding<Selector>,
    members: Vec<ObjectRef>,
    bounds: Cardinality,
    field: bool,
    selector: Selector,
}
impl MaterialWait {
    pub(super) fn revalidate(
        &self,
        game: &Game,
        execution: &Execution,
        members: &[ObjectRef],
    ) -> Result<()> {
        let eligible = game.material_candidates(execution, &self.selector, false)?;
        if !contains(
            self.bounds,
            Nat(u64::try_from(members.len()).map_err(invalid)?),
        ) || members.iter().any(|member| !eligible.contains(member))
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "reveal materials changed".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn inline_bind(&self, execution: &mut Execution, ids: &[String]) -> Result<()> {
        let handles = execution.inline_material_handles(&self.members, ids)?;
        let members = self.validate(&handles).map_err(|_error| {
            crate::EngineFailure::IllegalPlay("invalid evolution material selection".into())
        })?;
        self.bind(execution, members);
        Ok(())
    }

    pub(in crate::game::internal) fn validate(&self, handles: &[u64]) -> Result<Vec<ObjectRef>> {
        if !contains(
            self.bounds,
            Nat(u64::try_from(handles.len()).map_err(invalid)?),
        ) {
            return Err(invalid("material cardinality"));
        }
        let mut seen = BTreeSet::new();
        handles
            .iter()
            .map(|handle| {
                if !seen.insert(*handle) {
                    return Err(invalid("duplicate material handle"));
                }
                self.members
                    .get(usize::try_from(*handle).map_err(invalid)?)
                    .copied()
                    .ok_or_else(|| invalid("foreign material handle"))
            })
            .collect()
    }
    pub(in crate::game::internal) fn bind(
        &self,
        execution: &mut Execution,
        members: Vec<ObjectRef>,
    ) {
        if matches!(
            self.selector,
            Selector::Zone {
                zone: ZoneKind::Hand,
                ..
            }
        ) {
            execution
                .material_selections
                .insert(self.binding.key(), self.clone());
        }
        execution.slots.insert(
            self.binding.key(),
            Slot::Objects {
                members,
                on_field_at_capture: self.field,
            },
        );
    }
}

pub(super) const fn is_parameter(decision: &Decision) -> bool {
    matches!(
        decision.kind,
        DecisionKind::PlayParameter {
            slot: PlaySlot::CostMaterials,
            ..
        }
    )
}

fn zones(selector: &Selector) -> Result<Vec<ZoneKind>> {
    match selector {
        Selector::Zone {
            players,
            zone: zone @ (ZoneKind::Field | ZoneKind::Ex | ZoneKind::Cemetery),
            view: SelectionView::Current {},
            predicate,
            binding,
        } if matches!(
            players.relation,
            PlayerRelation::SelfPlayer | PlayerRelation::All
        ) && (players.relation != PlayerRelation::All || *zone == ZoneKind::Field)
            && players.relative_to == (PlayerRef::Controller {})
            && players.cardinality
                == (Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(if players.relation == PlayerRelation::All {
                        2
                    } else {
                        1
                    })),
                }) =>
        {
            search::check_predicate(predicate, *binding)?;
            Ok(vec![*zone])
        }
        Selector::Difference { input, except }
            if matches!(except.as_ref(),
            Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}]) =>
        {
            zones(input)
        }
        Selector::Union { sets } if sets.len() == 2 => {
            let mut result = Vec::new();
            for set in sets {
                if !matches!(set, Selector::Zone { players, .. }
                    if players.relation == PlayerRelation::SelfPlayer)
                {
                    return Err(unsupported("own material zone union"));
                }
                result.extend(zones(set)?);
            }
            if result != [ZoneKind::Field, ZoneKind::Ex]
                && result != [ZoneKind::Ex, ZoneKind::Field]
            {
                return Err(unsupported("field and EX material union"));
            }
            Ok(result)
        }
        _ => Err(unsupported("public material selector")),
    }
}

pub(super) fn check_reveal_parameter(decision: &Decision) -> Result<()> {
    if reveal_payment::parameter(decision).is_some() {
        let ChoiceDomain::Objects { constraint, .. } = &decision.domain else {
            return Err(unsupported("private hand materials"));
        };
        let [group] = constraint.groups.as_slice() else {
            return Err(unsupported("private hand material group"));
        };
        let Selector::Zone {
            predicate, binding, ..
        } = &group.selector
        else {
            return Err(unsupported("private hand materials"));
        };
        return search::check_predicate(predicate, *binding).map(|_checked| ());
    }
    Err(unsupported("private hand material parameter"))
}

pub(in crate::game::internal) fn check_parameter(decision: &Decision) -> Result<()> {
    if reveal_payment::parameter(decision).is_some() {
        return check_reveal_parameter(decision);
    }
    let ChoiceDomain::Objects {
        constraint,
        target_roles: None,
        ..
    } = &decision.domain
    else {
        return Err(unsupported("material choice domain"));
    };
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("one material group"));
    };
    if !decision.captures.is_empty()
        || decision.actor != (PlayerRef::Controller {})
        || decision.visibility != (Visibility::Public {})
        || decision.kind
            != (DecisionKind::PlayParameter {
                slot: PlaySlot::CostMaterials,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            })
        || !(constraint.cardinality
            == (Cardinality {
                minimum: Nat(0),
                maximum: None,
            })
            || (constraint.cardinality.minimum.0 > 0
                && constraint.cardinality.maximum == Some(constraint.cardinality.minimum)))
        || group.cardinality != constraint.cardinality
        || constraint.distinct_by.is_some()
        || constraint.disjoint != Disjointness::Disjoint
        || !constraint.aggregate_bounds.is_empty()
    {
        return Err(unsupported("strict single material group"));
    }
    if zones(&group.selector)?.len() > 1 && constraint.cardinality.minimum == Nat(0) {
        return Err(unsupported("fixed material zone union"));
    }
    Ok(())
}

pub(super) const fn cost_objects(action: &Action) -> Option<&Selector> {
    match action {
        Action::Banish { objects }
        | Action::Posture {
            objects,
            posture: Posture::Acted,
        } => Some(objects),
        Action::Move {
            objects,
            movement,
            reason: MoveReason::Put,
        } if (matches!(
            movement.to,
            MoveDestination::Owners {
                zone: OwnerZone::Cemetery | OwnerZone::Hand
            }
        ) || matches!(
            movement.to,
            MoveDestination::Shared {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Ex
                }
            }
        )) && movement.position.is_none()
            && movement.order.is_none()
            && movement.initial_posture.is_none()
            && movement.entry_effects.is_empty() =>
        {
            Some(objects)
        }
        _ => None,
    }
}

pub(super) fn check_cost(action: &Action, play: &PlaySpec) -> bool {
    let Some(objects) = cost_objects(action) else {
        return false;
    };
    if matches!(action, Action::Banish { .. })
        && matches!(objects, Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}])
    {
        return true;
    }
    let Selector::Bound { reference } = objects else {
        return false;
    };
    play.parameters.iter().any(|decision| {
        check_parameter(decision).is_ok()
            && matches!(&decision.domain, ChoiceDomain::Objects { binding, constraint, .. }
            if binding == reference && constraint.groups.first().is_some_and(|group|
                zones(&group.selector).is_ok_and(|zones|
                    (zones.len() == 1 || matches!(action, Action::Banish { .. }))
                    && zones.into_iter().all(|zone| cost_zone(action, zone)))))
    })
}

pub(super) fn source_field_cost(action: &Action) -> bool {
    matches!(cost_objects(action), Some(Selector::Objects { objects })
        if objects == &[ObjectOperand::SourceHolder {}])
        && matches!(
            action,
            Action::Posture { .. }
                | Action::Move {
                    movement: MoveSpec {
                        to: MoveDestination::Owners {
                            zone: OwnerZone::Cemetery
                        },
                        ..
                    },
                    ..
                }
        )
}

fn cost_zone(action: &Action, zone: ZoneKind) -> bool {
    match action {
        Action::Banish { .. } => true,
        Action::Posture { .. } => zone == ZoneKind::Field,
        Action::Move { movement, .. } if matches!(movement.to, MoveDestination::Shared { .. }) => {
            zone == ZoneKind::Field
        }
        Action::Move { .. } => zone != ZoneKind::Cemetery,
        _ => false,
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Material choice and payment share the play transaction."
)]
impl Game {
    fn material_candidates(
        &self,
        execution: &Execution,
        selector: &Selector,
        standing: bool,
    ) -> Result<Vec<ObjectRef>> {
        if let Selector::Union { sets } = selector {
            let mut members = Vec::new();
            for set in sets {
                for member in self.material_candidates(execution, set, standing)? {
                    if !members.contains(&member) {
                        members.push(member);
                    }
                }
            }
            return Ok(members);
        }
        if let Selector::Difference { input, .. } = selector {
            let source = execution.object(self, &ObjectOperand::SourceHolder {})?;
            return Ok(self
                .material_candidates(execution, input, standing)?
                .into_iter()
                .filter(|object| *object != source)
                .collect());
        }
        let Selector::Zone {
            players,
            zone,
            predicate,
            ..
        } = selector
        else {
            return Err(unsupported("material selector"));
        };
        let mut members = vec![];
        for player in execution.players(self, players)? {
            for id in self.zone_ids(
                seat_name(player)?,
                if *zone == ZoneKind::Field {
                    "field"
                } else if *zone == ZoneKind::Ex {
                    "ex"
                } else if *zone == ZoneKind::Hand {
                    "hand"
                } else {
                    "cemetery"
                },
            ) {
                if *zone == ZoneKind::Hand
                    && (self.object(&id)?.owner != seat_name(player)?
                        || self.object(&id)?.controller != seat_name(player)?)
                {
                    return Err(unsupported("foreign hand material owner"));
                }
                if standing && self.object(&id)?.state["acted"] != false {
                    continue;
                }
                if search::matches_candidate(self, &id, predicate)? {
                    members.push(execution.reference(self, &id)?);
                }
            }
        }
        Ok(members)
    }

    pub(super) fn internal_material_parameter(
        &self,
        execution: &mut Execution,
        decision: &Decision,
        standing: bool,
    ) -> Result<()> {
        check_parameter(decision)?;
        self.material_parameter(execution, decision, standing)
    }

    pub(super) fn internal_reveal_material_parameter(
        &self,
        execution: &mut Execution,
        decision: &Decision,
        additional: &PlaySpec,
    ) -> Result<()> {
        if !reveal_payment::payment(additional) || !additional.parameters.contains(decision) {
            return Err(unsupported(
                "private materials require mandatory revelation",
            ));
        }
        check_reveal_parameter(decision)?;
        self.material_parameter(execution, decision, false)
    }

    fn material_parameter(
        &self,
        execution: &mut Execution,
        decision: &Decision,
        standing: bool,
    ) -> Result<()> {
        let ChoiceDomain::Objects {
            constraint,
            binding,
            ..
        } = &decision.domain
        else {
            return Err(unsupported("material domain"));
        };
        let selector = &constraint
            .groups
            .first()
            .ok_or_else(|| invalid("material group"))?
            .selector;
        let members = self.material_candidates(execution, selector, standing)?;
        let count = usize::try_from(constraint.cardinality.minimum.0).map_err(invalid)?;
        if members.len() < count {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient materials".into(),
            ));
        }
        let public = members
            .iter()
            .enumerate()
            .map(|(index, member)| {
                let id = execution.object_id(*member)?;
                Ok(DistributionMember {
                    handle: u64::try_from(index).map_err(invalid)?,
                    player: player_id(&self.object(id)?.controller)?,
                    card: Some(self.object(id)?.card.clone()),
                    name: Some(self.card_name(id)?),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        execution.waiting = Some((
            DecisionView {
                token: DecisionToken {
                    execution: execution.id,
                    decision: decision.id,
                    epoch: 0,
                },
                actor: execution.context.controller,
                domain: InputDomain::CostMaterials(super::TargetDomain {
                    members: public,
                    cardinality: constraint.cardinality,
                    distinct_names: false,
                }),
            },
            Wait::MaterialParameter(MaterialWait {
                binding: *binding,
                members,
                bounds: constraint.cardinality,
                field: reveal_payment::parameter(decision).is_none()
                    && zones(selector)? == [ZoneKind::Field],
                selector: selector.clone(),
            }),
        ));
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Validation, freezing and movement form one atomic payment step."
    )]
    pub(super) fn internal_material_payment(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        objects: &Selector,
    ) -> Result<(Quantity, Vec<ObjectRef>, Vec<Characteristics>)> {
        let members = match objects {
            Selector::Bound { reference } => {
                let Some(Slot::Objects { members, .. }) = execution.slots.get(&reference.key())
                else {
                    return Err(invalid("missing material selection"));
                };
                members.clone()
            }
            Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}] => {
                vec![execution.object(self, &ObjectOperand::SourceHolder {})?]
            }
            _ => return Err(unsupported("bound or source materials")),
        };
        for member in &members {
            let id = execution.object_id(*member)?;
            let object = self.object(id)?;
            if object.generation != member.generation.0
                || !matches!(object.zone.as_str(), "field" | "ex" | "cemetery")
                || (object.controller != seat_name(execution.context.controller)?
                    && (!matches!(objects, Selector::Bound { .. }) || object.zone != "field"))
                || (matches!(instruction.action, Action::Posture { .. })
                    && matches!(objects, Selector::Bound { .. })
                    && object.zone != "field")
            {
                return Err(crate::EngineFailure::IllegalPlay(
                    "materials changed".into(),
                ));
            }
        }
        let characters = if let [member] = members.as_slice()
            && matches!(objects, Selector::Bound { .. })
            && matches!(&instruction.action, Action::Move { movement, reason: MoveReason::Put, .. }
                if movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery }))
            && self.object(execution.object_id(*member)?)?.zone == "field"
            && self
                .object_type(execution.object_id(*member)?)?
                .contains("フォロワー")
        {
            vec![self.internal_characteristics(execution.object_id(*member)?)?]
        } else {
            vec![]
        };
        if matches!(instruction.action, Action::Posture { .. }) {
            let ids = members
                .iter()
                .map(|member| execution.object_id(*member).map(str::to_owned))
                .collect::<Result<Vec<_>>>()?;
            if ids.iter().any(|id| {
                self.object(id)
                    .is_ok_and(|object| object.state["acted"] != false)
            }) {
                let reason = if matches!(objects, Selector::Objects { .. }) {
                    "whole-card posture cost requires a standing source"
                } else {
                    "material posture cost requires standing materials"
                };
                return Err(crate::EngineFailure::IllegalPlay(reason.into()));
            }
            if self.change_posture(&ids, true, &execution.frame().cause)? != ids {
                return Err(crate::EngineFailure::IllegalPlay(
                    "incomplete material posture cost".into(),
                ));
            }
            for member in &members {
                execution.facts.push(super::Fact::Posture(PostureEvent {
                    object: *member,
                    before: Posture::Standing,
                    after: Posture::Acted,
                }));
            }
        } else {
            let (destination, kind, method) = cost_destination(&instruction.action)?;
            if kind == ZoneKind::Ex {
                let available = 5_i64
                    .saturating_sub(self.zone_count(seat_name(execution.context.controller)?, "ex"))
                    .max(0);
                if i64::try_from(members.len()).map_err(invalid)? > available {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "insufficient EX capacity for cost".into(),
                    ));
                }
            }
            let (moved, _) = self.internal_move_members(
                execution,
                &members,
                destination,
                kind,
                method,
                false,
                None,
            )?;
            if moved.len() != members.len() || moved.iter().any(|result| result.after.is_none()) {
                return Err(crate::EngineFailure::IllegalPlay(
                    "incomplete material movement cost".into(),
                ));
            }
        }
        Ok((
            Quantity {
                amount: Int(i64::try_from(members.len()).map_err(invalid)?),
                unit: QuantityUnit::Cards,
            },
            members,
            characters,
        ))
    }
}

pub(super) fn posture_parameter(costs: &CostSpec, decision: &Decision) -> bool {
    let ChoiceDomain::Objects { binding, .. } = &decision.domain else {
        return false;
    };
    let CostSpec::Ordered { steps } = costs else {
        return false;
    };
    steps.iter().any(|step| {
        matches!(&step.instruction.action,
        Action::Posture { objects: Selector::Bound { reference }, .. } if reference == binding)
    })
}

pub(super) fn check_card(card: &WellFormedCardProgram) -> Result<()> {
    let quick_spell = card
        .card()
        .abilities
        .iter()
        .any(|ability| card_permission(&ability.node));
    for ability in &card.card().abilities {
        if let Some(play) = &ability.node.play {
            let materials = play
                .parameters
                .iter()
                .filter(|decision| is_parameter(decision))
                .collect::<Vec<_>>();
            if materials.is_empty() {
                continue;
            }
            if ability.node.kind == AbilityKind::Activated && is_printed_quick(&ability.node) {
                for decision in materials {
                    check_quick_parameter(decision)?;
                }
            }
            if quick_spell && ability.node.kind == AbilityKind::Spell {
                return Err(unsupported("material fee Quick Spell preflight"));
            }
        }
    }
    for ability in &card.card().abilities {
        if ability.node.play.as_ref().is_some_and(|play| {
            play.parameters.iter().any(|decision| {
                reveal_payment::parameter(decision).is_some()
                    && !reveal_payment::field_parameter(&ability.node, decision)
            })
        }) {
            return Err(unsupported("unconsumed field reveal parameter"));
        }
        if ability.node.play.as_ref().is_some_and(|play| {
            play.parameters
                .iter()
                .filter(|decision| is_parameter(decision))
                .count()
                > 1
        }) {
            return Err(unsupported("one public material group"));
        }
    }
    Ok(())
}

fn cost_destination(action: &Action) -> Result<(&'static str, ZoneKind, MoveMethod)> {
    match action {
        Action::Banish { .. } => Ok(("banish", ZoneKind::Banish, MoveMethod::Banish)),
        Action::Move { movement, .. }
            if movement.to
                == (MoveDestination::Owners {
                    zone: OwnerZone::Hand,
                }) =>
        {
            Ok(("hand", ZoneKind::Hand, MoveMethod::Put))
        }
        Action::Move { movement, .. }
            if matches!(
                movement.to,
                MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex
                    }
                }
            ) =>
        {
            Ok(("ex", ZoneKind::Ex, MoveMethod::Put))
        }
        Action::Move { .. } => Ok(("cemetery", ZoneKind::Cemetery, MoveMethod::Put)),
        _ => Err(unsupported("material payment action")),
    }
}

pub(super) fn check_quick_parameter(decision: &Decision) -> Result<()> {
    check_parameter(decision)?;
    let ChoiceDomain::Objects { constraint, .. } = &decision.domain else {
        return Err(unsupported("Quick public material domain"));
    };
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported("one Quick public material group"));
    };
    let selector = match &group.selector {
        Selector::Difference { input, .. } => input.as_ref(),
        selector => selector,
    };
    if constraint.cardinality.minimum == Nat(0)
        || constraint.cardinality.maximum != Some(constraint.cardinality.minimum)
        || !matches!(selector, Selector::Zone { players, zone: ZoneKind::Field | ZoneKind::Ex, .. }
            if players.relation == PlayerRelation::SelfPlayer)
    {
        return Err(unsupported("fixed own field or EX Quick materials"));
    }
    Ok(())
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Quick quotations reuse the authoritative material payment transaction."
)]
impl Game {
    pub(super) fn internal_quick_material_costs_available(
        &self,
        execution: &Execution,
        play: &PlaySpec,
    ) -> Result<bool> {
        let mut materials = play
            .parameters
            .iter()
            .filter(|decision| is_parameter(decision));
        let decision = materials
            .next()
            .ok_or_else(|| invalid("missing Quick materials"))?;
        if materials.next().is_some() {
            return Err(unsupported("one Quick public material group"));
        }
        check_quick_parameter(decision)?;
        let mut selection = execution.clone();
        match self.internal_material_parameter(
            &mut selection,
            decision,
            posture_parameter(&play.costs, decision),
        ) {
            Ok(()) => {}
            Err(crate::EngineFailure::IllegalPlay(_)) => return Ok(false),
            Err(error) => return Err(error),
        }
        let Some((question, Wait::MaterialParameter(wait))) = selection.waiting.take() else {
            return Err(invalid("missing Quick material quote"));
        };
        let InputDomain::CostMaterials(domain) = question.domain else {
            return Err(invalid("missing Quick material domain"));
        };
        let names = domain
            .members
            .iter()
            .map(|member| member.handle.to_string())
            .collect::<Vec<_>>();
        let count = usize::try_from(wait.bounds.minimum.0).map_err(invalid)?;
        // A single own Field or EX bounds candidates to five; unions stay rejected.
        for selected in subsets(&names, count, count) {
            let handles = selected
                .iter()
                .map(|handle| handle.parse::<u64>().map_err(invalid))
                .collect::<Result<Vec<_>>>()?;
            let mut trial = self.clone();
            let mut paid = execution.clone();
            trial.card_runtime.active_source = paid.ability;
            wait.bind(&mut paid, wait.validate(&handles)?);
            match trial.internal_card_payment(&mut paid, &play.costs, false) {
                Ok(()) => return Ok(true),
                Err(crate::EngineFailure::IllegalPlay(_)) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    }
}
