//! Mandatory hand revelation has a closed additional-cost entry.
use super::{
    Action, Binding, Cardinality, ChoiceDomain, CostSpec, Decision, DecisionKind, Disjointness,
    ExecutionRef, InputMode, InstructionContext, MoveDestination, MoveReason, Nat, OwnerZone,
    PlaySlot, PlaySpec, PlayerRef, PlayerRelation, Position, SelectionSnapshot, SelectionView,
    Selector, Visibility, ZoneKind,
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
