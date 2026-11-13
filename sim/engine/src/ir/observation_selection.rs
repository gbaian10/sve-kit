use super::observation::{self, Consumer, Position};
use super::{
    AggregateBound, Cardinality, ChoiceDomain, Comparison, Decision, DecisionKind, Declinable,
    Disjointness, Expr, Int, Nat, NumericProperty, ObjectProperty, PlayerRef, PlayerRelation,
    SelectionConstraint, Selector, Visibility,
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

/// Bounds shared by every choice among observed members, whoever may see them.
fn observed_constraint(decision: &Decision) -> Option<&SelectionConstraint> {
    let ChoiceDomain::Objects {
        constraint,
        target_roles: None,
        ..
    } = &decision.domain
    else {
        return None;
    };
    let DecisionKind::ResolutionChoice {
        may,
        cardinality,
        precondition: None,
    } = decision.kind
    else {
        return None;
    };
    (cardinality == constraint.cardinality
        && cardinality
            .maximum
            .is_some_and(|max| max.0 > 0 && cardinality.minimum.0 <= max.0)
        && (may == Declinable::Required || cardinality.minimum == Nat(0))
        && group_bounds(constraint, may)
        && match constraint.distinct_by {
            None => true,
            Some(ObjectProperty::Names) => {
                constraint.groups.len() == 1 && constraint.aggregate_bounds.is_empty()
            }
            _ => false,
        }
        && !matches!(
            card_totals(&constraint.aggregate_bounds),
            CardTotals::Unsupported
        )
        && constraint.disjoint == Disjointness::Disjoint)
        .then_some(constraint)
}

pub(crate) fn private_look_choice(decision: &Decision) -> bool {
    observed_constraint(decision).is_some_and(|constraint| {
        constraint.groups.iter().all(|group| matches!(&group.selector,
            Selector::Filter { input, .. } if observation::classify(input, Consumer::Choice, Position::Input).is_some()
        ))
    }) && decision.actor == (PlayerRef::Controller {})
        && matches!(&decision.visibility, Visibility::Only { players }
            if players.relation == PlayerRelation::SelfPlayer && players.relative_to == decision.actor)
}

/// Members of one public reveal: the observed choice bounds, shown to every player.
pub(crate) fn revealed_choice(decision: &Decision) -> bool {
    observed_constraint(decision).is_some_and(|constraint| {
        constraint.groups.iter().all(|group| matches!(&group.selector,
            Selector::Filter { input, .. } if matches!(input.as_ref(), Selector::ReceiptMembers {
                receipt: super::ObjectReceipt::Reveal { .. },
                projection: super::MemberProjection::Before,
            })
        ))
    }) && decision.visibility == (Visibility::Public {})
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

/// Current candidates from `holder`'s own hand, or a union of such branches ("手札のAかB").
pub(crate) fn own_hand_selector(selector: &Selector, holder: &PlayerRef) -> bool {
    if let Selector::Union { sets } = selector {
        return sets.len() > 1 && sets.iter().all(|set| own_hand_zone(set, holder));
    }
    own_hand_zone(selector, holder)
}

fn own_hand_zone(selector: &Selector, holder: &PlayerRef) -> bool {
    matches!(selector, Selector::Zone {
        players,
        zone: super::ZoneKind::Hand,
        view: super::SelectionView::Current {},
        ..
    } if held_by(players, holder))
}

/// Whether `players` names exactly one player: the controller, its opponent, or a procedure's
/// player.
pub(crate) fn single_holder(players: &super::PlayerSet) -> bool {
    match players.relation {
        PlayerRelation::SelfPlayer => super::perspective::readable(&players.relative_to),
        PlayerRelation::Opponents => players.relative_to == (PlayerRef::Controller {}),
        PlayerRelation::All | PlayerRelation::Bound => false,
    }
}

/// Whether `players` names exactly `holder`, read either directly or as the controller's opponent.
pub(crate) fn held_by(players: &super::PlayerSet, holder: &PlayerRef) -> bool {
    match players.relation {
        PlayerRelation::SelfPlayer => players.relative_to == *holder,
        PlayerRelation::Opponents => {
            players.relative_to == (PlayerRef::Controller {})
                && *holder == super::events::timing::opponent()
        }
        PlayerRelation::All | PlayerRelation::Bound => false,
    }
}

/// A hand choice authorizes only its controller to inspect current own-hand candidates.
pub(crate) fn private_hand_choice(decision: &Decision) -> bool {
    if matches!(
        decision.domain,
        ChoiceDomain::Objects {
            target_roles: Some(_),
            ..
        }
    ) {
        return super::hand_position::choice(decision).is_some();
    }
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
        && own_hand_selector(&group.selector, &decision.actor)
        && matches!(&decision.visibility, Visibility::Only { players }
            if players.relation == PlayerRelation::SelfPlayer && players.relative_to == decision.actor)
        && constraint.distinct_by.is_none()
        && constraint.aggregate_bounds.is_empty()
        && constraint.disjoint == Disjointness::Disjoint
}
