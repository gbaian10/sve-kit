//! Public prefix observation admits only the printed original-cost follower condition.
use super::{
    Action, Binding, Bool, CardKind, CharacteristicView, Comparison, Expr, Int, IntFrozen,
    IntOperation, KindRead, MemberProjection, Nat, NumberRead, NumericProperty, ObjectOperand,
    ObjectReceipt, ObjectRef, ObjectView, PlayerRef, Predicate, RevealUntilMembers, Selector,
};

pub(crate) fn filter(
    candidate: Binding<ObjectRef>,
    comparison: Comparison,
    threshold: Expr<Int>,
) -> Expr<Bool> {
    let object = ObjectView::Current {
        object: Box::new(ObjectOperand::Bound { binding: candidate }),
    };
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            Expr::Compute(Box::new(Predicate::KindEquals {
                left: Expr::Live(Box::new(KindRead::Kind {
                    object: object.clone(),
                    view: CharacteristicView::Effective,
                })),
                right: Expr::Literal(CardKind::Follower),
            })),
            Expr::Compute(Box::new(Predicate::CompareInt {
                left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                    information: NumberRead::Cost {
                        object,
                        view: CharacteristicView::Original,
                    },
                })),
                comparison,
                right: threshold,
            })),
        ],
    }))
}

pub(crate) fn cost_filter(action: &Action) -> Option<(Comparison, &Expr<Int>)> {
    let Action::RevealUntil {
        deck: PlayerRef::Controller {},
        candidate,
        predicate,
        required: Expr::Literal(Nat(required)),
    } = action
    else {
        return None;
    };
    let Expr::Compute(operation) = predicate else {
        return None;
    };
    let Predicate::And { predicates } = operation.as_ref() else {
        return None;
    };
    let Some(Expr::Compute(cost_test)) = predicates.get(1) else {
        return None;
    };
    let Predicate::CompareInt {
        comparison, right, ..
    } = cost_test.as_ref()
    else {
        return None;
    };
    let allowed = match (comparison, right) {
        (Comparison::LessEqual | Comparison::Greater, Expr::Literal(bound)) => bound.0 >= 0,
        (Comparison::GreaterEqual, Expr::Literal(bound)) => bound.0 == 0,
        (Comparison::Greater, Expr::Frozen(read)) => matches!(
            read.as_ref(),
            IntFrozen::PaidMaterialNumber {
                property: NumericProperty::OriginalCost,
                ..
            }
        ),
        (Comparison::Greater, Expr::Compute(read)) => matches!(
            read.as_ref(),
            IntOperation::Sum {
                objects: Selector::Bound { .. },
                property: NumericProperty::OriginalCost
            }
        ),
        _ => false,
    };
    (*required > 0 && allowed && *predicate == filter(*candidate, *comparison, right.clone()))
        .then_some((*comparison, right))
}

pub(crate) const fn matched(selector: &Selector) -> bool {
    matches!(
        selector,
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::RevealUntil {
                members: RevealUntilMembers::Matched,
                ..
            },
            projection: MemberProjection::Before,
        }
    )
}

pub(crate) fn receipt_action(action: &Action) -> bool {
    if super::shuffle::placement(action) == Some(super::shuffle::Placement::Bottom) {
        return true;
    }
    matches!(action, Action::Move { objects, movement, reason: super::MoveReason::Put }
        if matched(objects)
            && movement.to == (super::MoveDestination::Shared {
                zone: super::ZoneRef { player: PlayerRef::Controller {}, kind: super::ZoneKind::Field }
            })
            && movement.initial_posture.is_none()
            && movement.position.is_none()
            && movement.order.is_none()
            && movement.entry_effects.is_empty())
}
