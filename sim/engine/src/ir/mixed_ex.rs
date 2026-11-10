//! The closed shared-capacity batch needed by a departing card and one new token.
use super::{
    AbilityOrigin, Action, BatchCommit, BatchMembership, BatchViews, Control, Expr, Instruction,
    InstructionContext, KeywordId, MoveDestination, MoveReason, MovementEvent, Nat, ObjectOperand,
    OwnerZone, PlayerRef, Posture, ReceiptId, Selector, TokenDestination, TokenTemplate, ZoneKind,
};

pub(crate) struct Projection<'batch> {
    pub receipt: ReceiptId,
    pub movement: &'batch Instruction,
    pub creation: &'batch Instruction,
    pub template: &'batch str,
}

pub(crate) fn projection(control: &Control) -> Option<Projection<'_>> {
    let Control::Simultaneous { batch, receipt } = control else {
        return None;
    };
    if batch.membership != BatchMembership::AtBatchStart
        || batch.commit != BatchCommit::SharedCapacityAtomic
        || batch.event_views != BatchViews::BeforeAndAfter
        || !batch.choice_stages.is_empty()
    {
        return None;
    }
    let [movement, creation] = batch.members.as_slice() else {
        return None;
    };
    let (movement, creation) = (&movement.node, &creation.node);
    if movement.source != creation.source
        || !matches!(
            movement.source.origin,
            AbilityOrigin::Keyword {
                keyword: KeywordId::Lastword,
                ..
            }
        )
        || [movement, creation].iter().any(|instruction| {
            instruction.actor != (PlayerRef::Controller {})
                || instruction.context != InstructionContext::Effect
                || instruction.precondition.is_some()
        })
    {
        return None;
    }
    let Action::Move {
        objects: Selector::Objects { objects },
        movement: specification,
        reason: MoveReason::Put,
    } = &movement.action
    else {
        return None;
    };
    if !matches!(
        objects.as_slice(),
        [ObjectOperand::FollowEventMove {
            event: MovementEvent::LeaveField { .. }
        }]
    ) || specification.to
        != (MoveDestination::Owners {
            zone: OwnerZone::Ex,
        })
        || specification.position.is_some()
        || specification.order.is_some()
        || specification.initial_posture.is_some()
        || !specification.entry_effects.is_empty()
    {
        return None;
    }
    let Action::Token {
        tokens,
        destination: TokenDestination::Single {
            zone: Expr::Literal(zone),
        },
        initial_state: None,
        initial_posture: Posture::Standing,
        entry_effects,
        ..
    } = &creation.action
    else {
        return None;
    };
    let [token] = tokens.as_slice() else {
        return None;
    };
    let TokenTemplate::Fixed { template } = &token.template else {
        return None;
    };
    if token.count != Expr::Literal(Nat(1))
        || template.0.is_empty()
        || zone.player != (PlayerRef::Controller {})
        || zone.kind != ZoneKind::Ex
        || !entry_effects.is_empty()
    {
        return None;
    }
    Some(Projection {
        receipt: *receipt,
        movement,
        creation,
        template: &template.0,
    })
}
