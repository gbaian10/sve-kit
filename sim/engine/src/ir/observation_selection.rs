use super::{
    Cardinality, ChoiceDomain, Decision, DecisionKind, Declinable, Disjointness, MemberProjection,
    Nat, ObjectReceipt, PlayerRef, PlayerRelation, SelectionConstraint, Selector, Visibility,
};

pub(crate) fn private_look_choice(decision: &Decision) -> bool {
    let ChoiceDomain::Objects {
        constraint,
        target_roles: None,
        ..
    } = &decision.domain
    else {
        return false;
    };
    let DecisionKind::ResolutionChoice {
        may,
        cardinality,
        precondition: None,
    } = decision.kind
    else {
        return false;
    };
    cardinality == constraint.cardinality
        && cardinality.maximum.is_some_and(|max| max.0 > 0 && cardinality.minimum.0 <= max.0)
        && (may == Declinable::Required || cardinality.minimum == Nat(0))
        && group_bounds(constraint, may)
        && constraint.groups.iter().all(|group| matches!(&group.selector,
            Selector::Filter { input, .. } if matches!(input.as_ref(),
                Selector::ReceiptMembers { receipt: ObjectReceipt::Look { .. }, projection: MemberProjection::Before })
        ))
        && decision.actor == (PlayerRef::Controller {})
        && matches!(&decision.visibility, Visibility::Only { players }
            if players.relation == PlayerRelation::SelfPlayer && players.relative_to == decision.actor)
        && constraint.distinct_by.is_none()
        && constraint.aggregate_bounds.is_empty()
        && constraint.disjoint == Disjointness::Disjoint
}

fn group_bounds(constraint: &SelectionConstraint, may: Declinable) -> bool {
    match constraint.groups.as_slice() {
        [group] => group.cardinality == constraint.cardinality,
        [first, second] => {
            let one = Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(1)),
            };
            may == Declinable::MayDecline
                && constraint.cardinality
                    == (Cardinality {
                        minimum: Nat(0),
                        maximum: Some(Nat(2)),
                    })
                && first.cardinality == one
                && second.cardinality == one
        }
        _ => false,
    }
}

/// A hand choice authorizes only its controller to inspect current own-hand candidates.
pub(crate) fn private_hand_choice(decision: &Decision) -> bool {
    let ChoiceDomain::Objects {
        constraint,
        target_roles: None,
        ..
    } = &decision.domain
    else {
        return false;
    };
    let DecisionKind::ResolutionChoice {
        may,
        cardinality,
        precondition: None,
    } = decision.kind
    else {
        return false;
    };
    let [group] = constraint.groups.as_slice() else {
        return false;
    };
    cardinality == constraint.cardinality
        && group.cardinality == cardinality
        && cardinality
            .maximum
            .is_none_or(|max| max.0 > 0 && cardinality.minimum.0 <= max.0)
        && (may == Declinable::Required || cardinality.minimum == Nat(0))
        && matches!(&group.selector, Selector::Zone {
            players, zone: super::ZoneKind::Hand, view: super::SelectionView::Current {}, ..
        } if players.relation == PlayerRelation::SelfPlayer
            && players.relative_to == (PlayerRef::Controller {}))
        && decision.actor == (PlayerRef::Controller {})
        && matches!(&decision.visibility, Visibility::Only { players }
            if players.relation == PlayerRelation::SelfPlayer && players.relative_to == decision.actor)
        && constraint.distinct_by.is_none()
        && constraint.aggregate_bounds.is_empty()
        && constraint.disjoint == Disjointness::Disjoint
}
