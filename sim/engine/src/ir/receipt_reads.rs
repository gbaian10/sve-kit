//! Producer authority for the immutable printing retained by a field movement.

use super::{
    Action, MoveDestination, MoveReason, OwnerZone, PlayerRef, PlayerRelation, SelectionView,
    Selector, ZoneKind,
};

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
