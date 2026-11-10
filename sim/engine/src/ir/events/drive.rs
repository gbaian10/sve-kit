//! Drive observers distinguish a completed check from an accepted trigger effect.
use crate::ir::{
    Binding, Bool, DriveCheckedEvent, DriveTriggerEvent, EventPattern, Expr, ObjectFrozen,
    ObjectRef, PlayerFrozen, PlayerRef, Predicate,
};

pub(crate) fn checked(event: Binding<DriveCheckedEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::PlayerEquals {
        left: Expr::Frozen(Box::new(PlayerFrozen::DriveCheckedPlayer { event })),
        right: Expr::Literal(PlayerRef::Controller {}),
    }))
}
pub(crate) fn triggered(event: Binding<DriveTriggerEvent>, own_turn: bool) -> Expr<Bool> {
    let player = Expr::Compute(Box::new(Predicate::PlayerEquals {
        left: Expr::Frozen(Box::new(PlayerFrozen::DriveTriggerPlayer { event })),
        right: Expr::Literal(PlayerRef::Controller {}),
    }));
    if own_turn {
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![player, super::timing::turn_predicate(true)],
        }))
    } else {
        player
    }
}
pub(crate) fn checked_binding(pattern: &EventPattern) -> Option<Binding<DriveCheckedEvent>> {
    if let EventPattern::DriveChecked { binding, predicate } = pattern
        && *predicate == checked(*binding)
    {
        Some(*binding)
    } else {
        None
    }
}
pub(crate) fn holder(event: Binding<DriveCheckedEvent>) -> Expr<ObjectRef> {
    Expr::Frozen(Box::new(ObjectFrozen::DriveCheckedHolder { event }))
}
pub(crate) fn trigger_filter(pattern: &EventPattern) -> Option<bool> {
    let EventPattern::DriveTrigger { binding, predicate } = pattern else {
        return None;
    };
    [false, true]
        .into_iter()
        .find(|own| *predicate == triggered(*binding, *own))
}
