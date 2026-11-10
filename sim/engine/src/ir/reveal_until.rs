//! Public prefix observation admits only the printed original-cost follower condition.
use super::{
    Action, Binding, Bool, CardKind, CharacteristicView, Comparison, Expr, Int, IntOperation,
    KindRead, MemberProjection, Nat, NumberRead, ObjectOperand, ObjectReceipt, ObjectRef,
    ObjectView, PlayerRef, Predicate, RevealUntilMembers, Selector,
};

pub(crate) fn filter(candidate: Binding<ObjectRef>, maximum: Int) -> Expr<Bool> {
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
                comparison: Comparison::LessEqual,
                right: Expr::Literal(maximum),
            })),
        ],
    }))
}

pub(crate) fn action(action: &Action) -> bool {
    let Action::RevealUntil {
        deck: PlayerRef::Controller {},
        candidate,
        predicate,
        required: Expr::Literal(Nat(required)),
    } = action
    else {
        return false;
    };
    let Expr::Compute(operation) = predicate else {
        return false;
    };
    let Predicate::And { predicates } = operation.as_ref() else {
        return false;
    };
    let Some(Expr::Compute(comparison)) = predicates.get(1) else {
        return false;
    };
    let Predicate::CompareInt {
        right: Expr::Literal(maximum),
        ..
    } = comparison.as_ref()
    else {
        return false;
    };
    *required > 0 && maximum.0 >= 0 && *predicate == filter(*candidate, *maximum)
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
