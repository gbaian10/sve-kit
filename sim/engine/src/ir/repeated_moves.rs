//! A movement loop retains a typed receipt per iteration and rechecks its live termination.
use super::{
    Action, Control, Expr, IterationCheck, MemberProjection, MoveDestination, MoveReason,
    MovementReceipt, Nat, Node, ObjectReceipt, OwnerZone, PlayerRef, Position, Predicate, Program,
    ReceiptExport, ReceiptLocation, RepeatBody, ScopeId, Selector, SliceDirection, Termination,
    ZoneKind,
};

pub(crate) struct Projection<'control> {
    pub(crate) body: &'control Program,
    pub(crate) termination: &'control Termination,
    pub(crate) check: IterationCheck,
    pub(crate) scope: ScopeId,
    pub(crate) export: &'control ReceiptExport,
    pub(crate) to_ex: bool,
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
    let [
        export @ ReceiptExport::Move {
            source,
            iteration,
            collect,
        },
    ] = receipts.as_slice()
    else {
        return None;
    };
    let (step, before) = body.steps.split_last()?;
    let Node::Instruction { instruction } = &step.node else {
        return None;
    };
    let Action::Move {
        objects:
            Selector::Ordered {
                zone,
                start: Position::Top {},
                count: Expr::Literal(Nat(n)),
                direction: SliceDirection::Forward,
            },
        movement,
        reason: MoveReason::Put,
    } = &instruction.action
    else {
        return None;
    };
    if zone.kind != ZoneKind::Deck
        || zone.player != (PlayerRef::Controller {})
        || *scope == body.scope
        || !matches!(source.location(), ReceiptLocation::Local { id } if *id == instruction.receipt)
    {
        return None;
    }
    let to_ex = movement.to
        == (MoveDestination::Owners {
            zone: OwnerZone::Ex,
        });
    let mill = movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery })
        && *n > 0
        && collect.is_none()
        && *check == IterationCheck::AfterIteration
        && matches!(termination, Termination::While { predicate: Expr::Compute(predicate) }
            if matches!(predicate.as_ref(), Predicate::ReceiptAny {
                receipt: ObjectReceipt::Movement { receipt: MovementReceipt::Move { receipt } },
                projection: MemberProjection::Before,
                ..
            } if matches!(receipt.location(), ReceiptLocation::Iteration { binding } if binding == iteration)))
        // Each successful round consumes deck members. The preceding stat changes cannot refill it.
        && before.iter().all(|prior| matches!(&prior.node, Node::Instruction { instruction: prior_action }
            if matches!(prior_action.action, Action::Modify { .. })));
    ((to_ex && *n == 1 && before.is_empty()) || mill).then_some(Projection {
        body,
        termination,
        check: *check,
        scope: *scope,
        export,
        to_ex,
    })
}
