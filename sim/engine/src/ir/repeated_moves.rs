//! A movement loop retains a typed receipt per iteration and rechecks its live termination.
use super::{
    Action, Control, Expr, IterationCheck, MoveDestination, MoveReason, Nat, Node, OwnerZone,
    PlayerRef, Position, Program, ReceiptExport, ReceiptLocation, RepeatBody, ScopeId, Selector,
    SliceDirection, Termination, ZoneKind,
};

pub(crate) struct Projection<'control> {
    pub(crate) body: &'control Program,
    pub(crate) termination: &'control Termination,
    pub(crate) check: IterationCheck,
    pub(crate) scope: ScopeId,
    pub(crate) export: &'control ReceiptExport,
}

pub(crate) fn projection(control: &Control) -> Option<Projection<'_>> {
    let Control::Repeat {
        body: RepeatBody::Program { body },
        termination: termination @ (Termination::Until { .. } | Termination::While { .. }),
        check,
        scope,
        receipts,
    } = control
    else {
        return None;
    };
    let [export @ ReceiptExport::Move { source, .. }] = receipts.as_slice() else {
        return None;
    };
    let [step] = body.steps.as_slice() else {
        return None;
    };
    let Node::Instruction { instruction } = &step.node else {
        return None;
    };
    (matches!(&instruction.action, Action::Move {
        objects: Selector::Ordered { zone, start: Position::Top {}, count: Expr::Literal(Nat(1)), direction: SliceDirection::Forward },
        movement, reason: MoveReason::Put,
    } if zone.kind == ZoneKind::Deck && zone.player == (PlayerRef::Controller {}) && matches!(movement.to, MoveDestination::Owners { zone: OwnerZone::Ex }))
        && *scope != body.scope
        && matches!(source.location(), ReceiptLocation::Local { id } if *id == instruction.receipt))
    .then_some(Projection {
        body,
        termination,
        check: *check,
        scope: *scope,
        export,
    })
}
