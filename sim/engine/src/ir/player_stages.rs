//! Nonchoice player procedures cross every player at each instruction boundary.
use super::{
    Action, Binding, Cardinality, Control, Expr, InstructionContext, Int, IntOperation,
    MemberProjection, MovementReceipt, Nat, NatRead, Node, ObjectReceipt, PlayerOrder, PlayerRef,
    PlayerRelation, PlayerSet, PlayerStage, RandomnessPolicy, ReceiptId, ReceiptLocation, Selector,
    ShuffleScope, StageMode, StepwiseMode, TextQuantifier,
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

/// Every player in turn order runs each stage body, choices included, before the next player
/// starts that stage; the body is checked and executed by ordinary rules for that player.
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
            stage.mode == (StageMode::ChoiceProcedure {}) && !stage.body.steps.is_empty()
        }))
    .then_some((players, *player, stages))
}
