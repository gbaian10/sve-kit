use super::super::{Execution, Fact, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::player_stages;
use crate::ir::{
    Control, DestroyOutcome, Expr, LkiReason, MemberProjection, MoveMethod, MovementReceipt, Nat,
    NatOperation, Node, ObjectOperand, ObjectReceipt, ObjectView, PlayerOrder, PlayerRead,
    PlayerRef, PlayerRelation, Predicate, ReceiptLocation, ReceiptReference, Role, Selector,
    StageMode, TextQuantifier, ZoneKind,
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

pub(super) fn destroyed_controller(
    execution: &Execution,
    game: &super::super::Game,
    receipt: &ReceiptReference<DestroyOutcome>,
    player: &PlayerRef,
) -> Result<u64> {
    let ReceiptLocation::Local { id } = receipt.location() else {
        return Err(unsupported("local destroy result"));
    };
    let Some(InstructionOutcome::Destroy(outcome)) = execution.receipts.get(id) else {
        return Err(invalid("missing destroy result"));
    };
    let player = execution.player(game, player)?;
    outcome.successful.iter().try_fold(0_u64, |count, member| {
        let before = member
            .before
            .ok_or_else(|| invalid("destroy result missing Before"))?;
        let movement = execution
            .facts
            .iter()
            .find_map(|fact| match fact {
                Fact::Movement(fact)
                    if fact.method == MoveMethod::Destroy
                        && fact.movement.before == before.object =>
                {
                    Some(fact)
                }
                _ => None,
            })
            .ok_or_else(|| invalid("destroy result missing departure fact"))?;
        if movement.from.kind != ZoneKind::Field {
            return Err(invalid("destroy result did not leave field"));
        }
        count
            .checked_add(u64::from(movement.controller_before == player))
            .ok_or_else(|| invalid("token count overflow"))
    })
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
