//! Direct leader life changes share the ordinary life mutation and trigger producer.

use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::EngineFailure;
use crate::ir::{Action, Instruction, Int, LifeChangeEvent, NumericChange, Quantity, QuantityUnit};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Leader life changes use the same authoritative Game mutation boundary."
)]
impl Game {
    pub(super) fn internal_pay_life(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        delta: i64,
    ) -> Result<Quantity> {
        let amount = delta
            .checked_neg()
            .ok_or_else(|| invalid("life cost overflow"))?;
        let life = self
            .player(seat_name(execution.context.controller)?)?
            .leader
            .get("life")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| invalid("invalid leader life"))?;
        if life < amount {
            return Err(EngineFailure::IllegalPlay(
                "insufficient leader life".into(),
            ));
        }
        self.internal_life(execution, instruction)?;
        Ok(Quantity {
            amount: Int(amount),
            unit: QuantityUnit::Life,
        })
    }

    pub(super) fn internal_life(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
    ) -> Result<()> {
        let Action::Life { player, change } = &instruction.action else {
            return Err(unsupported("life action"));
        };
        let player = execution.player(self, player)?;
        let seat = seat_name(player)?;
        let before = self
            .player(seat)?
            .leader
            .get("life")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| invalid("invalid leader life"))?;
        let after = match change {
            NumericChange::Add { amount } => before
                .checked_add(execution.integer(self, amount)?.0)
                .ok_or_else(|| invalid("leader life overflow"))?,
            NumericChange::Set { amount } => execution.integer(self, amount)?.0,
        };
        let delta = after
            .checked_sub(before)
            .ok_or_else(|| invalid("leader life delta overflow"))?;
        let outcome = LifeChangeEvent {
            subject: player,
            before: Int(before),
            after: Int(after),
            delta: Int(delta),
        };
        if delta != 0 {
            self.change_life(seat, after)?;
            self.life_change_triggers(
                &[(seat.to_owned(), before, after)],
                &execution.frame().cause,
            )?;
            execution.facts.push(Fact::LifeChange(outcome));
        }
        execution
            .receipts
            .insert(instruction.receipt, InstructionOutcome::Life(outcome));
        Ok(())
    }
}
