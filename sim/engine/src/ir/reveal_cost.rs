//! Hand revelation has closed additional and field-ability cost entries.
use super::optional_payment;
use super::{
    Ability, AbilityKind, Action, Binding, Cardinality, ChoiceDomain, CostSpec, Decision,
    DecisionKind, Disjointness, ExecutionRef, Expr, InputMode, InstructionContext, Int,
    MoveDestination, MoveReason, Nat, ObjectOperand, OwnerZone, PlaySlot, PlaySpec, PlayerRef,
    PlayerRelation, Position, Posture, Resource, ResourceOperation, SelectionSnapshot,
    SelectionView, Selector, Visibility, ZoneKind,
};

pub(crate) fn parameter(decision: &Decision) -> Option<(Binding<Selector>, Cardinality)> {
    let ChoiceDomain::Objects {
        constraint,
        binding,
        target_roles: None,
    } = &decision.domain
    else {
        return None;
    };
    let [group] = constraint.groups.as_slice() else {
        return None;
    };
    let Selector::Zone {
        players,
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        ..
    } = &group.selector
    else {
        return None;
    };
    let count = constraint.cardinality;
    (matches!(count.minimum.0, 1 | 2)
        && count.maximum == Some(count.minimum)
        && group.cardinality == count
        && constraint.disjoint == Disjointness::Disjoint
        && constraint.distinct_by.is_none()
        && constraint.aggregate_bounds.is_empty()
        && players.relation == PlayerRelation::SelfPlayer
        && players.relative_to == (PlayerRef::Controller {})
        && players.cardinality
            == (Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            })
        && decision.actor == (PlayerRef::Controller {})
        && decision.visibility
            == (Visibility::Only {
                players: players.clone(),
            })
        && decision.captures.is_empty()
        && decision.kind
            == (DecisionKind::PlayParameter {
                slot: PlaySlot::CostMaterials,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            }))
    .then_some((*binding, count))
}

pub(crate) fn selected(action: &Action, parameters: &[Decision]) -> bool {
    let Action::Reveal {
        objects: Selector::Bound { reference },
    } = action
    else {
        return false;
    };
    parameters
        .iter()
        .any(|decision| parameter(decision).is_some_and(|(binding, _)| binding == *reference))
}

fn field_steps(costs: &CostSpec, binding: Binding<Selector>) -> bool {
    let CostSpec::Ordered { steps } = costs else {
        return false;
    };
    let mut reveals = 0_u8;
    for step in steps {
        let action = &step.instruction.action;
        if matches!(action, Action::Reveal { objects: Selector::Bound { reference } } if *reference == binding)
        {
            reveals = reveals.saturating_add(1);
            continue;
        }
        let pp = matches!(action, Action::Resource { player: PlayerRef::Controller {}, resource: Resource::PpCurrent,
            operation: ResourceOperation::Pay, amount: Expr::Literal(Int(amount)) } if *amount >= 0);
        let act = matches!(action, Action::Posture { objects: Selector::Objects { objects }, posture: Posture::Acted }
            if objects == &[ObjectOperand::SourceHolder {}]);
        if !pp && !act {
            return false;
        }
    }
    reveals == 1
}

pub(crate) fn field_parameter(ability: &Ability, decision: &Decision) -> bool {
    let Some((binding, _)) = parameter(decision) else {
        return false;
    };
    let Some(play) = &ability.play else {
        return false;
    };
    ability.active_zones == [ZoneKind::Field]
        && matches!(
            ability.kind,
            AbilityKind::Activated | AbilityKind::Triggered
        )
        && play.parameters.contains(decision)
        && (field_steps(&play.costs, binding)
            || ability.body.as_ref().is_some_and(|body| {
                optional_payment::required(body)
                    && optional_payment::declaration(body)
                        .is_some_and(|(costs, _)| field_steps(costs, binding))
            }))
}

pub(crate) fn field_cost(ability: &Ability, costs: &CostSpec) -> bool {
    let Some(play) = &ability.play else {
        return false;
    };
    play.parameters.iter().any(|decision| {
        field_parameter(ability, decision)
            && parameter(decision).is_some_and(|(binding, _)| field_steps(costs, binding))
            && (*costs == play.costs
                || ability.body.as_ref().is_some_and(|body| {
                    optional_payment::declaration(body).is_some_and(|(head, _)| head == costs)
                }))
    })
}

pub(crate) fn payment(play: &PlaySpec) -> bool {
    let [decision] = play.parameters.as_slice() else {
        return false;
    };
    let Some((binding, count)) = parameter(decision) else {
        return false;
    };
    let CostSpec::Ordered { steps } = &play.costs else {
        return false;
    };
    let [first, rest @ ..] = steps.as_slice() else {
        return false;
    };
    let selected = Selector::Bound { reference: binding };
    if play.limit.is_some()
        || !play.captures.is_empty()
        || first.instruction.action
            != (Action::Reveal {
                objects: selected.clone(),
            })
        || !steps.iter().all(|step| {
            step.materials.is_none()
                && step.keyword.is_none()
                && step.instruction.context == InstructionContext::Cost
                && step.instruction.actor == (PlayerRef::Controller {})
                && step.instruction.source == first.instruction.source
                && step.instruction.precondition.is_none()
        })
    {
        return false;
    }
    match rest {
        [] => true,
        [step] if count.minimum == Nat(1) => matches!(&step.instruction.action,
            Action::Move { objects, movement, reason: MoveReason::Put }
                if *objects == selected
                    && movement.to == (MoveDestination::Owners { zone: OwnerZone::Deck })
                    && movement.position == Some(Position::Bottom {})
                    && movement.order.is_none() && movement.initial_posture.is_none()
                    && movement.entry_effects.is_empty()),
        _ => false,
    }
}

pub(crate) fn bottom(action: &Action) -> Option<&Selector> {
    let Action::Move {
        objects: objects @ Selector::Bound { .. },
        movement,
        reason: MoveReason::Put,
    } = action
    else {
        return None;
    };
    (movement.to
        == (MoveDestination::Owners {
            zone: OwnerZone::Deck,
        })
        && movement.position == Some(Position::Bottom {})
        && movement.order.is_none()
        && movement.initial_posture.is_none()
        && movement.entry_effects.is_empty())
    .then_some(objects)
}

pub(crate) fn objects(action: &Action) -> Option<&Selector> {
    if let Action::Reveal { objects } = action {
        Some(objects)
    } else {
        bottom(action)
    }
}
