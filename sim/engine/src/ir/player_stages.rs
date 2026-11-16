//! Player procedures preserve stage boundaries and each player's own receipts.
use super::{
    Action, Binding, Cardinality, Control, Expr, Instruction, InstructionContext, Int,
    IntOperation, MemberProjection, MoveDestination, MoveReason, MovementReceipt, Nat, NatRead,
    Node, ObjectReceipt, OwnerZone, PlayerOrder, PlayerRef, PlayerRelation, PlayerSet, PlayerStage,
    Program, RandomnessPolicy, ReceiptId, ReceiptLocation, Selector, ShuffleScope, StageMode,
    StepwiseMode, TextQuantifier,
};

pub(crate) fn procedure(
    control: &Control,
) -> Option<(&PlayerSet, Binding<PlayerRef>, &[PlayerStage])> {
    let Control::EachPlayer {
        players,
        player,
        stages,
        choice_order: PlayerOrder::TurnPlayerFirst,
    } = control
    else {
        return None;
    };
    if players.relation != PlayerRelation::All
        || players.relative_to != (PlayerRef::Controller {})
        || players.cardinality
            != (Cardinality {
                minimum: Nat(0),
                maximum: None,
            })
        || players.text_quantifier != TextQuantifier::All
        || stages.is_empty()
    {
        return None;
    }
    let actor = PlayerRef::Bound { binding: *player };
    let mut returns = vec![];
    for stage in stages {
        if stage.mode != (StageMode::NonChoiceSimultaneous {})
            || stage.body.steps.is_empty()
            || !super::shuffle::closed(&stage.body.steps)
        {
            return None;
        }
        for step in &stage.body.steps {
            let Node::Instruction { instruction } = &step.node else {
                return None;
            };
            if instruction.actor != actor
                || instruction.context != InstructionContext::Effect
                || instruction.precondition.is_some()
            {
                return None;
            }
            let action = &instruction.action;
            let valid = if let Action::Draw {
                player: drawing_player,
                count,
                mode,
            } = action
            {
                *drawing_player == actor
                    && *mode == StepwiseMode::Exact
                    && (matches!(count, Expr::Literal(Int(count)) if (0..=10_000).contains(count))
                        || returned_share(count).is_some_and(|id| returns.contains(&id)))
            } else if super::shuffle::whole_return(action) == Some(&actor) {
                returns.push(instruction.receipt);
                true
            } else {
                matches!(action, Action::Shuffle {
                    deck,
                    scope: ShuffleScope::FullZone {},
                    randomness: RandomnessPolicy::FreshRequest,
                } if *deck == actor)
            };
            if !valid {
                return None;
            }
        }
    }
    Some((players, *player, stages))
}

/// Inside a player stage, counting an earlier whole return reads only the current player's
/// share: the cards that player actually returned to their own deck.
pub(crate) fn returned_share(count: &Expr<Int>) -> Option<ReceiptId> {
    let Expr::Compute(operation) = count else {
        return None;
    };
    let IntOperation::FromNat {
        value: Expr::Live(read),
    } = operation.as_ref()
    else {
        return None;
    };
    let NatRead::ZoneCount {
        selector:
            Selector::ReceiptMembers {
                receipt:
                    ObjectReceipt::Movement {
                        receipt: MovementReceipt::Move { receipt },
                    },
                projection: MemberProjection::Before,
            },
    } = read.as_ref()
    else {
        return None;
    };
    match receipt.location() {
        ReceiptLocation::Local { id } => Some(*id),
        ReceiptLocation::Iteration { .. } => None,
    }
}

/// Choice stages let each player finish before the next starts; a closed nonchoice Look
/// remainder stage instead commits all players' shares together.
pub(crate) fn choice_procedure(
    control: &Control,
) -> Option<(&PlayerSet, Binding<PlayerRef>, &[PlayerStage])> {
    let Control::EachPlayer {
        players,
        player,
        stages,
        choice_order: PlayerOrder::TurnPlayerFirst,
    } = control
    else {
        return None;
    };
    (players.relation == PlayerRelation::All
        && players.relative_to == (PlayerRef::Controller {})
        && players.cardinality
            == (Cardinality {
                minimum: Nat(0),
                maximum: None,
            })
        && players.text_quantifier == TextQuantifier::All
        && !stages.is_empty()
        && stages.iter().all(|stage| {
            (stage.mode == (StageMode::ChoiceProcedure {}) && !stage.body.steps.is_empty())
                || (stage.mode == (StageMode::NonChoiceSimultaneous {})
                    && look_remainder(&stage.body, *player).is_some())
        }))
    .then_some((players, *player, stages))
}

/// A nonchoice stage may commit every player's private Look remainder together, after each
/// player's own receipt and current deck authority have been checked.
pub(crate) fn look_remainder(body: &Program, player: Binding<PlayerRef>) -> Option<&Instruction> {
    let [step] = body.steps.as_slice() else {
        return None;
    };
    let Node::Instruction { instruction } = &step.node else {
        return None;
    };
    let Action::Move {
        objects,
        movement,
        reason: MoveReason::Put,
    } = &instruction.action
    else {
        return None;
    };
    let input = if let Selector::Difference { input, except } = objects {
        if !matches!(
            except.as_ref(),
            Selector::ReceiptMembers {
                receipt: ObjectReceipt::Movement {
                    receipt: MovementReceipt::Move { .. }
                },
                projection: MemberProjection::Before,
            }
        ) {
            return None;
        }
        input.as_ref()
    } else {
        objects
    };
    (instruction.actor == (PlayerRef::Bound { binding: player })
        && instruction.context == InstructionContext::Effect
        && instruction.precondition.is_none()
        && movement.to
            == (MoveDestination::Owners {
                zone: OwnerZone::Cemetery,
            })
        && movement.initial_posture.is_none()
        && movement.entry_effects.is_empty()
        && movement.position.is_none()
        && movement.order.is_none()
        && matches!(
            input,
            Selector::ReceiptMembers {
                receipt: ObjectReceipt::Look { .. },
                projection: MemberProjection::Before
            }
        ))
    .then_some(instruction.as_ref())
}
