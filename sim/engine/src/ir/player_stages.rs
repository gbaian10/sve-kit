//! Nonchoice player procedures cross every player at each instruction boundary.
use super::{
    Action, Binding, Cardinality, Control, Expr, InstructionContext, Int, Nat, Node, PlayerOrder,
    PlayerRef, PlayerRelation, PlayerSet, PlayerStage, RandomnessPolicy, ShuffleScope, StageMode,
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
    let valid = stages.iter().all(|stage| {
        stage.mode == StageMode::NonChoiceSimultaneous {}
            && !stage.body.steps.is_empty()
            && super::shuffle::closed(&stage.body.steps)
            && stage.body.steps.iter().all(|step| {
                let Node::Instruction { instruction } = &step.node else {
                    return false;
                };
                if instruction.actor != actor
                    || instruction.context != InstructionContext::Effect
                    || instruction.precondition.is_some()
                {
                    return false;
                }
                matches!(&instruction.action, Action::Draw {
                        player: drawing_player,
                        count: Expr::Literal(Int(count)),
                        mode: StepwiseMode::Exact,
                    } if *drawing_player == actor && (0..=10_000).contains(count))
                    || matches!(&instruction.action, Action::Shuffle {
                        deck,
                        scope: ShuffleScope::FullZone {},
                        randomness: RandomnessPolicy::FreshRequest,
                    } if *deck == actor)
                    || super::shuffle::whole_return(&instruction.action) == Some(&actor)
            })
    });
    valid.then_some((players, *player, stages))
}
