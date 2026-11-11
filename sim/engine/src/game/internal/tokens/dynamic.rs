use super::super::{Result, unsupported};
use crate::ir::player_stages;
use crate::ir::{
    Control, DestroyOutcome, Expr, LkiReason, MemberProjection, MovementReceipt, Nat, NatOperation,
    Node, ObjectOperand, ObjectReceipt, ObjectView, PlayerOrder, PlayerRead, PlayerRef,
    PlayerRelation, Predicate, ReceiptLocation, ReceiptReference, Role, Selector, StageMode,
    TextQuantifier,
};

pub(super) fn projection(
    value: &Expr<Nat>,
) -> Option<(&ReceiptReference<DestroyOutcome>, &PlayerRef)> {
    let Expr::Compute(operation) = value else {
        return None;
    };
    let NatOperation::Count {
        objects:
            Selector::Filter {
                input,
                binding,
                predicate,
            },
    } = operation.as_ref()
    else {
        return None;
    };
    let Selector::ReceiptMembers {
        receipt:
            ObjectReceipt::Movement {
                receipt: MovementReceipt::Destroy { receipt },
            },
        projection: MemberProjection::Before,
    } = input.as_ref()
    else {
        return None;
    };
    let Expr::Compute(predicate) = predicate.as_ref() else {
        return None;
    };
    let Predicate::PlayerEquals {
        left: Expr::Live(read),
        right: Expr::Literal(player),
    } = predicate.as_ref()
    else {
        return None;
    };
    let PlayerRead::Role {
        object:
            ObjectView::LastKnown {
                object,
                reason: LkiReason::ZoneExit,
            },
        role: Role::Controller,
    } = read.as_ref()
    else {
        return None;
    };
    if !matches!(object.as_ref(), ObjectOperand::Bound { binding: object } if object == binding)
        || !matches!(receipt.location(), ReceiptLocation::Local { .. })
        || !matches!(player, PlayerRef::Controller {} | PlayerRef::Bound { .. })
    {
        return None;
    }
    Some((receipt, player))
}

pub(crate) fn check_each(control: &Control) -> Result<()> {
    if player_stages::procedure(control).is_some() {
        return Ok(());
    }
    let Control::EachPlayer {
        players,
        player,
        stages,
        choice_order: PlayerOrder::TurnPlayerFirst,
    } = control
    else {
        return Err(unsupported("each-player stage order"));
    };
    let [stage] = stages.as_slice() else {
        return Err(unsupported("one each-player stage"));
    };
    if players.relation != PlayerRelation::All
        || players.relative_to != (PlayerRef::Controller {})
        || players.cardinality.minimum != Nat(0)
        || players.cardinality.maximum.is_some()
        || players.text_quantifier != TextQuantifier::All
        || stage.mode != (StageMode::NonChoiceSimultaneous {})
    {
        return Err(unsupported("all-player stage"));
    }
    let [step] = stage.body.steps.as_slice() else {
        return Err(unsupported("single each-player instruction stage"));
    };
    let Node::Instruction { instruction } = &step.node else {
        return Err(unsupported("each-player instruction stage"));
    };
    if super::super::movement::player_banish(control).is_some() {
        return Ok(());
    }
    let Some((tokens, zone)) = super::specification(&instruction.action) else {
        return Err(unsupported("player token shape"));
    };
    let [token] = tokens else {
        return Err(unsupported("single-template player token stage"));
    };
    if zone.player != (PlayerRef::Bound { binding: *player })
        || instruction.actor != zone.player
        || !super::counts::CountPlan::compile(&token.count)?.matches_player(&zone.player)
    {
        return Err(unsupported(
            "token destination and counted player must match stage",
        ));
    }
    Ok(())
}
