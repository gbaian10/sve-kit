//! Flip payments retain their own strict material choice, independent of target permissions.
use super::{
    Action, Binding, Bool, BoolRead, BooleanState, CardKind, CharacteristicView, ChoiceDomain,
    Decision, Expr, KindRead, Located, Node, ObjectOperand, ObjectRef, ObjectView, PlayerRef,
    PlayerRelation, Predicate, SelectionView, Selector, ZoneKind,
};

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "This recognizer admits only the closed flip-payment grammar."
)]
fn state(value: &Expr<Bool>, candidate: Binding<ObjectRef>) -> Option<bool> {
    match value {
        Expr::Live(read)
            if matches!(read.as_ref(), BoolRead::State {
            object: ObjectView::Current { object }, property: BooleanState::FaceUp,
        } if object.as_ref() == &(ObjectOperand::Bound { binding: candidate })) =>
        {
            Some(true)
        }
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::Not { predicate } if matches!(predicate, Expr::Live(_)) => {
                state(predicate, candidate).map(|up| !up)
            }
            Predicate::And { predicates } => {
                predicates.iter().find_map(|part| state(part, candidate))
            }
            _ => None,
        },
        _ => None,
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "This recognizer admits only the closed flip-payment grammar."
)]
fn follower(value: &Expr<Bool>, candidate: Binding<ObjectRef>) -> bool {
    matches!(value, Expr::Compute(operation) if match operation.as_ref() {
        Predicate::KindEquals { left: Expr::Live(read), right: Expr::Literal(CardKind::EvolveFollower) } => matches!(read.as_ref(), KindRead::Kind {
            object: ObjectView::Current { object }, view: CharacteristicView::Effective,
        } if object.as_ref() == &(ObjectOperand::Bound { binding: candidate })),
        Predicate::And { predicates } => predicates.iter().any(|part| follower(part, candidate)),
        _ => false,
    })
}

/// The orientation conjunct cannot be bypassed by an OR branch.
pub(crate) fn selector(selector: &Selector) -> Option<bool> {
    let Selector::Zone {
        zone: ZoneKind::Evolve,
        players,
        binding,
        predicate,
        view: SelectionView::Current {},
    } = selector
    else {
        return None;
    };
    if players.relation != PlayerRelation::SelfPlayer
        || players.relative_to != (PlayerRef::Controller {})
    {
        return None;
    }
    state(predicate, *binding).filter(|before| !before || follower(predicate, *binding))
}

pub(crate) fn material(decision: &Decision) -> Option<(&Selector, bool)> {
    let ChoiceDomain::Objects {
        constraint,
        target_roles: None,
        ..
    } = &decision.domain
    else {
        return None;
    };
    let [group] = constraint.groups.as_slice() else {
        return None;
    };
    Some((&group.selector, selector(&group.selector)?))
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "This recognizer admits only the closed flip-payment grammar."
)]
pub(crate) fn payment(action: &Action, decision: &Decision) -> bool {
    let Some((_, before)) = material(decision) else {
        return false;
    };
    let ChoiceDomain::Objects { binding, .. } = &decision.domain else {
        return false;
    };
    let expected = Selector::Bound {
        reference: *binding,
    };
    match action {
        Action::FlipUp { objects, faces } => !before && *objects == expected && faces.is_empty(),
        Action::FlipDown { objects } => before && *objects == expected,
        _ => false,
    }
}

pub(crate) fn private_material(decision: &Decision) -> bool {
    private_face_down(decision)
        && matches!(
            decision.kind,
            super::DecisionKind::PlayParameter {
                slot: super::PlaySlot::CostMaterials,
                ..
            }
        )
}

pub(crate) fn private_resolution(decision: &Decision) -> bool {
    let super::DecisionKind::ResolutionChoice {
        may: super::Declinable::Required,
        cardinality,
        precondition: None,
    } = decision.kind
    else {
        return false;
    };
    let Some((
        Selector::Zone {
            predicate, binding, ..
        },
        false,
    )) = material(decision)
    else {
        return false;
    };
    if !follower(predicate, *binding)
        || cardinality.minimum != super::Nat(0)
        || cardinality.maximum.is_none_or(|n| n.0 == 0)
        || !private_face_down(decision)
    {
        return false;
    }
    matches!(&decision.domain, ChoiceDomain::Objects { constraint, .. }
        if constraint.cardinality == cardinality
            && constraint.groups.first().is_some_and(|group| group.cardinality == cardinality)
            && constraint.distinct_by.is_none()
            && constraint.aggregate_bounds.is_empty()
            && constraint.disjoint == super::Disjointness::Disjoint)
}

fn private_face_down(decision: &Decision) -> bool {
    let Some((Selector::Zone { players, .. }, false)) = material(decision) else {
        return false;
    };
    decision.actor == (PlayerRef::Controller {})
        && decision.visibility
            == (super::Visibility::Only {
                players: players.clone(),
            })
}

/// Keep the private choice in the same scope as its single use.
pub(crate) fn closed_resolution(steps: &[Located<Node>]) -> bool {
    for step in steps {
        if let Node::Decision { decision } = &step.node
            && private_resolution(decision)
        {
            return matches!(steps, [first, second]
                if matches!(&first.node, Node::Decision { decision: first } if first == decision)
                    && matches!(&second.node, Node::Instruction { instruction }
                        if instruction.precondition.is_none()
                            && payment(&instruction.action, decision)));
        }
    }
    true
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "This recognizer admits only the closed flip-payment grammar."
)]
pub(crate) const fn bound_action(action: &Action) -> Option<bool> {
    match action {
        Action::FlipUp {
            objects: Selector::Bound { .. },
            faces,
        } if faces.is_empty() => Some(true),
        Action::FlipDown {
            objects: Selector::Bound { .. },
        } => Some(false),
        _ => None,
    }
}
