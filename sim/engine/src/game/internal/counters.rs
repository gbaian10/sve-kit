//! Fixed card counters share the object's authoritative counter map.

#![expect(
    clippy::indexing_slicing,
    reason = "Counter writes address a constructed object state map."
)]

use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, CounterChangeEvent, CounterKind, CounterOperation, Expr, Holder, Instruction, Int, Nat,
    ObjectOperand, Quantity, QuantityUnit, RuntimeHolder,
};
use serde_json::json;

pub(super) fn check_action(action: &Action) -> Result<()> {
    if let Action::Counter {
        holder,
        operation,
        amount,
        ..
    } = action
        && (!matches!(
            holder,
            Holder::Object {
                object: ObjectOperand::SourceHolder {}
            }
        ) || !matches!(
            operation,
            CounterOperation::Add {} | CounterOperation::Remove {}
        ) || !matches!(amount, Expr::Literal(Nat(n)) if i64::try_from(*n).is_ok()))
    {
        return Err(unsupported("fixed source counter change"));
    }
    Ok(())
}

pub(super) fn is_cost(action: &Action) -> bool {
    matches!(
        action,
        Action::Counter {
            operation: CounterOperation::Remove {},
            ..
        }
    ) && check_action(action).is_ok()
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Counter reads use the same fixed source generation as changes."
)]
impl Execution {
    pub(super) fn counter(&self, game: &Game, holder: &Holder, kind: &CounterKind) -> Result<Nat> {
        let Holder::Object {
            object: ObjectOperand::SourceHolder {},
        } = holder
        else {
            return Err(unsupported("source counter read"));
        };
        let reference = self.object(game, &ObjectOperand::SourceHolder {})?;
        let object = game.object(self.object_id(reference)?)?;
        if object.generation != reference.generation.0 {
            return Err(unsupported("stale counter holder requires a snapshot"));
        }
        let key = game.catalog.keyword_id(&kind.0);
        let value = object
            .state
            .get("counters")
            .and_then(|counters| counters.get(&key));
        let count = value.map_or(Ok(0), |value| {
            value
                .as_i64()
                .filter(|count| *count >= 0)
                .ok_or_else(|| invalid("invalid card counter"))
        })?;
        Ok(Nat(u64::try_from(count).map_err(invalid)?))
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Counter costs and effects mutate one authoritative object."
)]
impl Game {
    pub(super) fn internal_counter(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        payment: bool,
    ) -> Result<Quantity> {
        check_action(&instruction.action)?;
        let Action::Counter {
            holder,
            kind,
            operation,
            amount: Expr::Literal(amount),
        } = &instruction.action
        else {
            return Err(unsupported("fixed counter amount"));
        };
        let before = execution.counter(self, holder, kind)?;
        let reference = execution.object(self, &ObjectOperand::SourceHolder {})?;
        let id = execution.object_id(reference)?.to_owned();
        if !matches!(self.object(&id)?.zone.as_str(), "field" | "ex") {
            return Err(unsupported("counter holder must be in Field or EX"));
        }
        if payment && before.0 < amount.0 {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient card counters".into(),
            ));
        }
        let after = match operation {
            CounterOperation::Add {} => before
                .0
                .checked_add(amount.0)
                .filter(|value| i64::try_from(*value).is_ok())
                .ok_or_else(|| invalid("counter overflow"))?,
            CounterOperation::Remove {} => before.0.saturating_sub(amount.0),
            CounterOperation::Move { .. } => return Err(unsupported("counter movement")),
        };
        let delta = i64::try_from(after)
            .map_err(invalid)?
            .checked_sub(i64::try_from(before.0).map_err(invalid)?)
            .ok_or_else(|| invalid("counter delta overflow"))?;
        let key = self.catalog.keyword_id(&kind.0);
        if delta != 0 {
            self.object_mut(&id)?.state["counters"][&key] = json!(after);
            let group = self.group();
            let name = self.catalog.keyword_name(&key);
            let name = if name.is_empty() { key.as_str() } else { name };
            self.emit(
                json!({"kind":"カウンター","object":id,"name":name,"delta":delta}),
                &execution.frame().cause,
                group,
            );
        }
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::Counter(CounterChangeEvent {
                holder: RuntimeHolder::Object { object: reference },
                kind: CounterKind(key),
                before,
                after: Nat(after),
                delta: Int(delta),
            }),
        );
        Ok(Quantity {
            amount: Int(delta.saturating_abs()),
            unit: QuantityUnit::Counters,
        })
    }
}
