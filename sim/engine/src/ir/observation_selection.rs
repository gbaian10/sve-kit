use super::{
    AggregateBound, Cardinality, ChoiceDomain, Comparison, Decision, DecisionKind, Declinable,
    Disjointness, Expr, Int, MemberProjection, Nat, NumericProperty, ObjectReceipt, PlayerRef,
    PlayerRelation, SelectionConstraint, Selector, Visibility,
};

/// Aggregate bounds a choice of concrete cards admits besides dynamic counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardTotals {
    Unbounded,
    /// 元のコストの合計がN以下: a fixed cap on the chosen cards' printed costs.
    OriginalCostAtMost(i64),
    Unsupported,
}

pub(crate) const fn card_totals(bounds: &[AggregateBound]) -> CardTotals {
    match bounds {
        [] => CardTotals::Unbounded,
        [
            AggregateBound {
                property: NumericProperty::OriginalCost,
                comparison: Comparison::LessEqual,
                value: Expr::Literal(Int(limit)),
            },
        ] if *limit >= 0 => CardTotals::OriginalCostAtMost(*limit),
        _ => CardTotals::Unsupported,
    }
}

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
        && !matches!(
            card_totals(&constraint.aggregate_bounds),
            CardTotals::Unsupported
        )
        && constraint.disjoint == Disjointness::Disjoint
}

fn group_bounds(constraint: &SelectionConstraint, may: Declinable) -> bool {
    match constraint.groups.as_slice() {
        [group] => group.cardinality == constraint.cardinality,
        groups @ ([_, _] | [_, _, _]) => {
            let maximum = if groups.len() == 2 { Nat(2) } else { Nat(3) };
            let one = Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(1)),
            };
            may == Declinable::MayDecline
                && constraint.cardinality
                    == (Cardinality {
                        minimum: Nat(0),
                        maximum: Some(maximum),
                    })
                && groups.iter().all(|group| group.cardinality == one)
        }
        _ => false,
    }
}

/// Current own-hand candidates, or a union of such branches ("手札のAかB").
pub(crate) fn own_hand_selector(selector: &Selector) -> bool {
    if let Selector::Union { sets } = selector {
        return sets.len() > 1 && sets.iter().all(own_hand_zone);
    }
    own_hand_zone(selector)
}

fn own_hand_zone(selector: &Selector) -> bool {
    matches!(selector, Selector::Zone {
        players, zone: super::ZoneKind::Hand, view: super::SelectionView::Current {}, ..
    } if players.relation == PlayerRelation::SelfPlayer
        && players.relative_to == (PlayerRef::Controller {}))
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
        && own_hand_selector(&group.selector)
        && decision.actor == (PlayerRef::Controller {})
        && matches!(&decision.visibility, Visibility::Only { players }
            if players.relation == PlayerRelation::SelfPlayer && players.relative_to == decision.actor)
        && constraint.distinct_by.is_none()
        && constraint.aggregate_bounds.is_empty()
        && constraint.disjoint == Disjointness::Disjoint
}
