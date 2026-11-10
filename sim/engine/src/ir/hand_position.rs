//! One private card and its deck edge share the existing selection-role partition.
use super::observation_selection::private_hand_choice;
use super::{
    Action, Bool, Cardinality, ChoiceDomain, Control, Decision, Expr, InstructionContext,
    MoveDestination, MoveReason, Nat, Node, OwnerZone, PlayerRef, Position, Program, RepeatBody,
    RoleCardinality, Selector, TargetRoleSpec,
};

pub(crate) fn choice(decision: &Decision) -> Option<&TargetRoleSpec> {
    if !decision.captures.is_empty() {
        return None;
    }
    let ChoiceDomain::Objects {
        constraint,
        binding,
        target_roles: Some(spec),
    } = &decision.domain
    else {
        return None;
    };
    let [group] = constraint.groups.as_slice() else {
        return None;
    };
    if constraint.cardinality
        != (Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        })
        || !matches!(&group.selector, Selector::Zone { predicate, .. } if **predicate == Expr::Literal(Bool(true)))
    {
        return None;
    }
    let [top, bottom] = spec.roles.as_slice() else {
        return None;
    };
    let optional_one = RoleCardinality::Fixed {
        cardinality: Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(1)),
        },
    };
    if top.role != Nat(1)
        || bottom.role != Nat(2)
        || top.cardinality != optional_one
        || bottom.cardinality != optional_one
        || top.binding == bottom.binding
        || top.binding == *binding
        || bottom.binding == *binding
    {
        return None;
    }
    let mut plain = decision.clone();
    let ChoiceDomain::Objects { target_roles, .. } = &mut plain.domain else {
        return None;
    };
    *target_roles = None;
    private_hand_choice(&plain).then_some(spec)
}

fn positioned_move(node: &Node, binding: super::Binding<Selector>, position: Position) -> bool {
    matches!(node, Node::Instruction { instruction }
        if instruction.actor == (PlayerRef::Controller {})
        && instruction.context == InstructionContext::Effect && instruction.precondition.is_none()
        && matches!(&instruction.action, Action::Move { objects: Selector::Bound { reference }, movement, reason: MoveReason::Put }
            if *reference == binding
            && movement.to == (MoveDestination::Owners { zone: OwnerZone::Deck })
            && movement.position == Some(position) && movement.order.is_none()
            && movement.initial_posture.is_none() && movement.entry_effects.is_empty()))
}

pub(crate) fn valid(program: &Program) -> bool {
    for (index, step) in program.steps.iter().enumerate() {
        if let Node::Decision { decision } = &step.node
            && matches!(&decision.domain, ChoiceDomain::Objects { target_roles: Some(spec), .. }
                if spec.roles.iter().any(|role| matches!(role.cardinality, RoleCardinality::Fixed { .. })))
        {
            let Some(spec) = choice(decision) else {
                return false;
            };
            let Some(top) = index
                .checked_add(1)
                .and_then(|next| program.steps.get(next))
            else {
                return false;
            };
            let Some(bottom) = index
                .checked_add(2)
                .and_then(|next| program.steps.get(next))
            else {
                return false;
            };
            let [top_role, bottom_role] = spec.roles.as_slice() else {
                return false;
            };
            if !positioned_move(&top.node, top_role.binding, Position::Top {})
                || !positioned_move(&bottom.node, bottom_role.binding, Position::Bottom {})
            {
                return false;
            }
        }
        let accepted = match &step.node {
            Node::Control {
                control:
                    Control::Sequence { body }
                    | Control::May { body, .. }
                    | Control::EachObject { body, .. }
                    | Control::Pay { on_paid: body, .. }
                    | Control::Repeat {
                        body: RepeatBody::Program { body },
                        ..
                    },
            } => valid(body),
            Node::Control {
                control:
                    Control::If {
                        then_body,
                        else_body,
                        ..
                    },
            } => valid(then_body) && else_body.as_ref().is_none_or(valid),
            Node::Control {
                control: Control::Mode { modes, .. },
            } => modes.iter().all(|mode| valid(&mode.body)),
            Node::Control {
                control: Control::EachPlayer { stages, .. },
            } => stages.iter().all(|stage| valid(&stage.body)),
            Node::Instruction { .. }
            | Node::Control { .. }
            | Node::Capture { .. }
            | Node::Decision { .. } => true,
        };
        if !accepted {
            return false;
        }
    }
    true
}
