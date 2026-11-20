//! Producer authority for the immutable printing retained by a field movement.

use super::{
    Action, MoveDestination, MoveReason, MovementReceipt, OwnerZone, PlayerRef, PlayerRelation,
    ReceiptId, ReceiptLocation, ReceiptReference, SelectionView, Selector, ZoneKind,
};

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Other movement outcomes do not record payment materials."
)]
pub(crate) const fn paid_movement_id(receipt: &MovementReceipt) -> Option<ReceiptId> {
    match receipt {
        MovementReceipt::Move { receipt } => local(receipt),
        MovementReceipt::Banish { receipt } => local(receipt),
        _ => None,
    }
}

const fn local<T>(receipt: &ReceiptReference<T>) -> Option<ReceiptId> {
    match receipt.location() {
        ReceiptLocation::Local { id } => Some(*id),
        ReceiptLocation::Iteration { .. } => None,
    }
}

pub(crate) fn field_cemetery(action: &Action) -> bool {
    matches!(action, Action::Move { objects, movement, reason: MoveReason::Put }
        if own_field(objects)
            && movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery })
            && movement.position.is_none()
            && movement.order.is_none()
            && movement.initial_posture.is_none()
            && movement.entry_effects.is_empty())
}

fn own_field(objects: &Selector) -> bool {
    if let Selector::Difference { input, .. } = objects {
        return own_field(input);
    }
    matches!(objects, Selector::Zone {
        players, zone: ZoneKind::Field, view: SelectionView::Current {}, ..
    } if players.relation == PlayerRelation::SelfPlayer
        && players.relative_to == (PlayerRef::Controller {}))
}
