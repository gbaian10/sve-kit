//! Private hand materials have closed revelation and field-transfer cost entries.
use super::optional_payment;
use super::quick::is_printed_quick;
use super::{
    Ability, AbilityKind, Action, Binding, Cardinality, ChoiceDomain, Control, CostSpec, Decision,
    DecisionKind, Disjointness, ExecutionRef, Expr, InputMode, InstructionContext, Int, Mode,
    ModeChoiceKind, MoveDestination, MoveReason, Nat, Node, ObjectOperand, OwnerZone, PlaySlot,
    PlaySpec, PlayerRef, PlayerRelation, Position, Posture, Resource, ResourceOperation,
    SelectionSnapshot, SelectionView, Selector, Visibility, ZoneKind, ZoneRef,
};
use core::slice;

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
    let Some(Selector::Bound { reference }) = objects(action) else {
        return false;
    };
    parameters.iter().any(|decision| {
        parameter(decision).is_some_and(|(binding, count)| {
            binding == *reference
                && (matches!(action, Action::Reveal { .. }) || count.minimum == Nat(1))
        })
    })
}

fn field_steps(costs: &CostSpec, binding: Binding<Selector>, count: Nat) -> bool {
    let CostSpec::Ordered { steps } = costs else {
        return false;
    };
    let mut selections = 0_u8;
    for step in steps {
        let action = &step.instruction.action;
        if matches!(action, Action::Reveal { objects: Selector::Bound { reference } } if *reference == binding)
            || (count == Nat(1)
                && matches!(transfer(action).or_else(|| banish(action)), Some(Selector::Bound { reference }) if *reference == binding))
        {
            selections = selections.saturating_add(1);
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
    selections == 1
}

pub(crate) fn field_parameter(ability: &Ability, decision: &Decision) -> bool {
    let Some((binding, count)) = parameter(decision) else {
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
        && (field_steps(&play.costs, binding, count.minimum)
            || ability.body.as_ref().is_some_and(|body| {
                optional_payment::required(body)
                    && optional_payment::declaration(body)
                        .is_some_and(|(costs, _)| field_steps(costs, binding, count.minimum))
            }))
}

pub(crate) fn field_cost(ability: &Ability, costs: &CostSpec) -> bool {
    let Some(play) = &ability.play else {
        return false;
    };
    let header = play.parameters.iter().any(|decision| {
        field_parameter(ability, decision)
            && parameter(decision)
                .is_some_and(|(binding, count)| field_steps(costs, binding, count.minimum))
            && (*costs == play.costs
                || ability.body.as_ref().is_some_and(|body| {
                    optional_payment::declaration(body).is_some_and(|(head, _)| head == costs)
                }))
    });
    header
        || field_modes(ability).is_some_and(|modes| {
            modes.iter().any(|mode| {
                field_mode(ability, mode)
                    && optional_payment::mode_declaration(&mode.body)
                        .is_some_and(|(fee, _, _)| fee == costs)
            })
        })
}

fn field_modes(ability: &Ability) -> Option<&[Mode]> {
    if is_printed_quick(ability)
        || ability.active_zones != [ZoneKind::Field]
        || !matches!(
            ability.kind,
            AbilityKind::Activated | AbilityKind::Triggered
        )
    {
        return None;
    }
    let [step] = ability.body.as_ref()?.steps.as_slice() else {
        return None;
    };
    let Node::Control {
        control:
            Control::Mode {
                count: Expr::Literal(Nat(1)),
                choice_kind: ModeChoiceKind::ChoiceKeyword,
                modes,
                ..
            },
    } = &step.node
    else {
        return None;
    };
    Some(modes)
}

pub(crate) fn field_mode(ability: &Ability, mode: &Mode) -> bool {
    field_modes(ability).is_some_and(|modes| modes.contains(mode))
        && mode
            .parameters
            .first()
            .is_some_and(|decision| mode_parameter(mode, decision))
}

pub(crate) fn mode_parameter(mode: &Mode, decision: &Decision) -> bool {
    if mode.parameters.as_slice() != slice::from_ref(decision)
        || mode.costs != (CostSpec::Ordered { steps: vec![] })
    {
        return false;
    }
    let Some((binding, count)) = parameter(decision) else {
        return false;
    };
    let Some((CostSpec::Ordered { steps }, _, _)) = optional_payment::mode_declaration(&mode.body)
    else {
        return false;
    };
    let [step] = steps.as_slice() else {
        return false;
    };
    count.minimum == Nat(1)
        && matches!(banish(&step.instruction.action), Some(Selector::Bound { reference }) if *reference == binding)
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
        transfer(action).or_else(|| banish(action))
    }
}

pub(crate) const fn banish(action: &Action) -> Option<&Selector> {
    let Action::Banish {
        objects: objects @ Selector::Bound { .. },
    } = action
    else {
        return None;
    };
    Some(objects)
}

pub(crate) fn transfer(action: &Action) -> Option<&Selector> {
    if let Some(objects) = bottom(action) {
        return Some(objects);
    }
    let Action::Move {
        objects: objects @ Selector::Bound { .. },
        movement,
        reason: MoveReason::Put,
    } = action
    else {
        return None;
    };
    (movement.to
        == (MoveDestination::Shared {
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Ex,
            },
        })
        && movement.position.is_none()
        && movement.order.is_none()
        && movement.initial_posture.is_none()
        && movement.entry_effects.is_empty())
    .then_some(objects)
}
