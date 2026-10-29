use super::super::{Execution, Fact, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Control, DestroyOutcome, Expr, LkiReason, MemberProjection, MoveMethod, MovementReceipt, Nat,
    NatOperation, Node, ObjectOperand, ObjectReceipt, ObjectView, PlayerOrder, PlayerRead,
    PlayerRef, PlayerRelation, Predicate, ReceiptLocation, ReceiptReference, Role, Selector,
    StageMode, ZoneKind,
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

pub(super) fn count(
    execution: &Execution,
    game: &super::super::Game,
    value: &Expr<Nat>,
) -> Result<u64> {
    if let Expr::Literal(Nat(count)) = value {
        return Ok(*count);
    }
    let (receipt, player) =
        projection(value).ok_or_else(|| unsupported("destroyed-controller token count"))?;
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
    let Control::EachPlayer {
        players,
        player,
        stages,
        choice_order: PlayerOrder::TurnPlayerFirst,
    } = control
    else {
        return Err(unsupported("token player stage order"));
    };
    let [stage] = stages.as_slice() else {
        return Err(unsupported("one token player stage"));
    };
    if players.relation != PlayerRelation::All
        || players.relative_to != (PlayerRef::Controller {})
        || stage.mode != (StageMode::NonChoiceSimultaneous {})
    {
        return Err(unsupported("all-player token stage"));
    }
    let [step] = stage.body.steps.as_slice() else {
        return Err(unsupported("single token instruction stage"));
    };
    let Node::Instruction { instruction } = &step.node else {
        return Err(unsupported("token instruction stage"));
    };
    let Some((token, zone)) = super::specification(&instruction.action) else {
        return Err(unsupported("player token shape"));
    };
    if zone.player != (PlayerRef::Bound { binding: *player })
        || instruction.actor != zone.player
        || projection(&token.count).is_some_and(|(_, counted)| counted != &zone.player)
    {
        return Err(unsupported(
            "token destination and counted player must match stage",
        ));
    }
    Ok(())
}
