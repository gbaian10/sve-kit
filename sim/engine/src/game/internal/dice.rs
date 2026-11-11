//! A roll accepts one final value after its instruction-local optional rerolls.
use super::{Execution, Game, InputDomain, Result, Slot, Wait, invalid, seat_name, unsupported};
use crate::game::player::observation::DiceHistory;
use crate::ir::PlayerId;
use crate::ir::dice as ir_dice;
use crate::ir::{
    Action, Comparison, Control, DecisionId, DiceEvent, DiceOperation, DieFace, Expr, Instruction,
    Nat, Node, PlayerRef, RollId, Termination,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::game) struct DiceHistoryFact {
    turn: u64,
    player: u32,
    value: u64,
}

#[derive(Clone, Debug)]
pub(super) struct RollWait {
    instruction: Instruction,
    event: DiceEvent,
    remaining: u64,
}

pub(super) fn check_instruction(instruction: &Instruction) -> Result<()> {
    check_action(&instruction.action)?;
    if matches!(instruction.action, Action::Dice { .. })
        && instruction.actor != (PlayerRef::Controller {})
    {
        return Err(unsupported("dice requires the controller as roller"));
    }
    Ok(())
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    if matches!(
        action,
        Action::Dice {
            operation: DiceOperation::Reroll { .. },
            ..
        }
    ) {
        return Err(unsupported("rerolls belong to the original roll procedure"));
    }
    Ok(())
}

pub(super) fn history_count(
    execution: &Execution,
    game: &Game,
    filter: Option<(Comparison, Nat)>,
) -> Result<Nat> {
    let turn = game.state.schedule.current()?;
    let count = game
        .state
        .ir_dice_history
        .iter()
        .filter(|fact| {
            fact.turn == turn
                && fact.player == execution.context.controller.seat
                && filter.is_none_or(|(comparison, value)| match comparison {
                    Comparison::Equal => fact.value == value.0,
                    Comparison::LessEqual => fact.value <= value.0,
                    Comparison::GreaterEqual => fact.value >= value.0,
                    Comparison::Less | Comparison::Greater | Comparison::NotEqual => false,
                })
        })
        .count();
    Ok(Nat(u64::try_from(count).map_err(invalid)?))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Dice shares the game's seeded stream and transaction boundary."
)]
impl Game {
    pub(super) fn observed_dice_history(&self) -> Result<Vec<DiceHistory>> {
        use crate::game::player::observation::{DiceHistory, viewer};
        self.state
            .ir_dice_history
            .iter()
            .map(|fact| {
                Ok(DiceHistory {
                    player: viewer(seat_name(PlayerId { seat: fact.player })?)?,
                    value: fact.value,
                })
            })
            .collect()
    }

    fn internal_die(&mut self) -> Result<DieFace> {
        match self.state.rng.bounded(6)? {
            0 => Ok(DieFace::One),
            1 => Ok(DieFace::Two),
            2 => Ok(DieFace::Three),
            3 => Ok(DieFace::Four),
            4 => Ok(DieFace::Five),
            5 => Ok(DieFace::Six),
            _ => Err(invalid("die outside its closed range")),
        }
    }

    pub(super) fn internal_repeat(execution: &mut Execution, control: Control) -> Result<()> {
        let (body, count) = ir_dice::repeat(&control).ok_or_else(|| unsupported("fixed repeat"))?;
        let Some(left) = count.checked_sub(1) else {
            return Ok(());
        };
        let body = body.clone();
        let mut next = control;
        if let Control::Repeat {
            termination:
                Termination::Count {
                    count: Expr::Literal(remaining),
                },
            ..
        } = &mut next
        {
            *remaining = Nat(left);
        }
        execution.todo.push_front(Node::Control { control: next });
        execution.prepend(body);
        Ok(())
    }

    pub(super) fn internal_dice(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        check_instruction(instruction)?;
        let Action::Dice {
            operation: DiceOperation::Roll {},
            ..
        } = instruction.action
        else {
            return Err(unsupported("roll action"));
        };
        let mut remaining = 0_u64;
        for holder in self.zone_ids(seat_name(execution.context.controller)?, "field") {
            let (card, _) = self.internal_effective_print(&holder)?;
            if !self.card_runtime.is_loaded(&card) {
                continue;
            }
            for ability in self.internal_printed_abilities(&holder)? {
                if let Some(times) = ir_dice::rerolls(&ability.node) {
                    remaining = remaining
                        .checked_add(times.0)
                        .ok_or_else(|| invalid("reroll count overflow"))?;
                }
            }
        }
        self.state.ir_epoch = self
            .state
            .ir_epoch
            .checked_add(1)
            .ok_or_else(|| invalid("roll identities exhausted"))?;
        let value = self.internal_die()?;
        self.internal_dice_continue(
            execution,
            RollWait {
                instruction: instruction.clone(),
                event: DiceEvent {
                    roller: execution.context.controller,
                    roll: RollId(self.state.ir_epoch),
                    value,
                    attempts: vec![value],
                    accepted_attempt: Nat(0),
                },
                remaining,
            },
        )
    }

    pub(super) fn internal_dice_answer(
        &mut self,
        execution: &mut Execution,
        mut wait: RollWait,
        reroll: bool,
    ) -> Result<()> {
        if reroll {
            wait.event.value = self.internal_die()?;
            wait.event.attempts.push(wait.event.value);
            wait.event.accepted_attempt.0 = wait
                .event
                .accepted_attempt
                .0
                .checked_add(1)
                .ok_or_else(|| invalid("die attempt overflow"))?;
            wait.remaining = wait
                .remaining
                .checked_sub(1)
                .ok_or_else(|| invalid("no reroll permission"))?;
            self.internal_dice_continue(execution, wait)
        } else {
            self.internal_accept_die(execution, &wait)
        }
    }

    fn internal_dice_continue(&mut self, execution: &mut Execution, wait: RollWait) -> Result<()> {
        if wait.remaining == 0 {
            return self.internal_accept_die(execution, &wait);
        }
        self.internal_wait(
            execution,
            DecisionId(wait.instruction.id.0),
            wait.event.roller,
            InputDomain::DiceReroll {
                value: Nat(ir_dice::value(wait.event.value)),
                remaining: Nat(wait.remaining),
            },
            Wait::Dice(Box::new(wait)),
        )
    }

    fn internal_accept_die(&mut self, execution: &mut Execution, wait: &RollWait) -> Result<()> {
        let Action::Dice { result, .. } = &wait.instruction.action else {
            return Err(invalid("missing dice result"));
        };
        let value = Nat(ir_dice::value(wait.event.value));
        execution.slots.insert(result.key(), Slot::Natural(value));
        let turn = self.state.schedule.current()?;
        self.state.ir_dice_history.retain(|fact| fact.turn == turn);
        self.state.ir_dice_history.push(DiceHistoryFact {
            turn,
            player: wait.event.roller.seat,
            value: value.0,
        });
        let group = self.group();
        self.emit(
            json!({"kind":"サイコロ","player":seat_name(wait.event.roller)?,"result":value.0}),
            &execution.frame().cause,
            group,
        );
        execution.record_fact(self, super::Fact::Dice(wait.event.clone()));
        self.internal_dice_event(&wait.event, execution)
    }
}
