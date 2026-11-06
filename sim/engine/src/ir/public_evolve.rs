//! Public evolve-deck selections require an affirmative face-up filter.

use super::{
    Binding, Bool, BoolRead, BooleanState, Expr, ObjectOperand, ObjectRef, ObjectView, PlayerRef,
    PlayerRelation, Predicate, SelectionView, Selector, ZoneKind,
};

fn requires_face_up(value: &Expr<Bool>, candidate: Binding<ObjectRef>) -> bool {
    match value {
        Expr::Live(read) => {
            matches!(read.as_ref(), BoolRead::State { object: ObjectView::Current { object }, property: BooleanState::FaceUp } if matches!(object.as_ref(), ObjectOperand::Bound { binding } if *binding == candidate))
        }
        Expr::Compute(operation) => {
            matches!(operation.as_ref(), Predicate::And { predicates } if predicates.iter().any(|part| requires_face_up(part, candidate)))
        }
        Expr::Literal(_) | Expr::Frozen(_) | Expr::Captured(_) | Expr::Bound(_) => false,
    }
}

pub(crate) fn selector(selector: &Selector) -> bool {
    matches!(selector, Selector::Zone { zone: ZoneKind::Evolve, players, binding, predicate, view: SelectionView::Current {} }
        if players.relation == PlayerRelation::SelfPlayer && players.relative_to == (PlayerRef::Controller {}) && requires_face_up(predicate, *binding))
}
