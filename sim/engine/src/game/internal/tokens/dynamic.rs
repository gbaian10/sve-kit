use super::super::{Execution, Fact, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::player_stages;
use crate::ir::{
    Control, DestroyOutcome, Expr, IntOperation, LkiReason, MemberProjection, MoveMethod,
    MovementReceipt, Nat, NatOperation, NatRead, Node, ObjectOperand, ObjectReceipt, ObjectView,
    PlayerOrder, PlayerRead, PlayerRef, PlayerRelation, Predicate, ReceiptLocation,
    ReceiptReference, Role, Selector, StageMode, TextQuantifier, ZoneKind,
};

pub(super) fn zone_count(value: &Expr<Nat>) -> Option<&Selector> {
    let Expr::Compute(operation) = value else {
        return None;
    };
    let NatOperation::CheckedInt {
        value: Expr::Compute(integer),
    } = operation.as_ref()
    else {
        return None;
    };
    let IntOperation::FromNat {
        value: Expr::Live(read),
    } = integer.as_ref()
    else {
        return None;
    };
    let NatRead::ZoneCount { selector } = read.as_ref() else {
        return None;
    };
    (super::super::card::selection::field_selector(selector).is_ok()
        || super::super::card::selection::public_zone_selector(selector).is_ok())
    .then_some(selector)
}

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

#[derive(Clone, Debug)]
pub(super) enum CountPlan {
    Literal(u64),
    PublicZone(Expr<Nat>),
    DestroyedController(ReceiptReference<DestroyOutcome>, PlayerRef),
}
impl CountPlan {
    pub(super) fn compile(value: &Expr<Nat>) -> Result<Self> {
        if let Expr::Literal(Nat(count)) = value {
            return Ok(Self::Literal(*count));
        }
        if zone_count(value).is_some() {
            return Ok(Self::PublicZone(value.clone()));
        }
        let (receipt, player) =
            projection(value).ok_or_else(|| unsupported("destroyed-controller token count"))?;
        Ok(Self::DestroyedController(receipt.clone(), player.clone()))
    }
    pub(super) fn evaluate(&self, execution: &Execution, game: &super::super::Game) -> Result<u64> {
        match self {
            Self::Literal(count) => Ok(*count),
            Self::PublicZone(value) => execution.natural(game, value).map(|value| value.0),
            Self::DestroyedController(receipt, player) => {
                destroyed_controller(execution, game, receipt, player)
            }
        }
    }
}

fn destroyed_controller(
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
        || projection(&token.count).is_some_and(|(_, counted)| counted != &zone.player)
    {
        return Err(unsupported(
            "token destination and counted player must match stage",
        ));
    }
    Ok(())
}
