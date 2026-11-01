//! Card counters share the object's authoritative counter map.

#![expect(
    clippy::indexing_slicing,
    reason = "Counter writes address a constructed object state map."
)]

use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, Binding, Control, CounterChangeEvent, CounterKind, CounterOperation, Expr, Holder,
    Instruction, Int, Nat, ObjectOperand, ObjectRef, Quantity, QuantityUnit, RuntimeHolder,
    Selector,
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
                object: ObjectOperand::SourceHolder {} | ObjectOperand::Bound { .. }
            }
        ) || !matches!(
            operation,
            CounterOperation::Add {} | CounterOperation::Remove {}
        ) || !matches!(amount, Expr::Literal(Nat(n)) if i64::try_from(*n).is_ok()))
    {
        return Err(unsupported("object counter change"));
    }
    Ok(())
}

pub(super) fn is_cost(action: &Action) -> bool {
    matches!(
        action,
        Action::Counter {
            operation: CounterOperation::Remove {},
            holder: Holder::Object {
                object: ObjectOperand::SourceHolder {}
            },
            ..
        }
    ) && check_action(action).is_ok()
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Counter reads retain the referenced object generation."
)]
impl Execution {
    pub(super) fn counter(&self, game: &Game, holder: &Holder, kind: &CounterKind) -> Result<Nat> {
        let Holder::Object { object: operand } = holder else {
            return Err(unsupported("source counter read"));
        };
        if !matches!(
            operand,
            ObjectOperand::SourceHolder {} | ObjectOperand::Bound { .. }
        ) {
            return Err(unsupported("counter read holder"));
        }
        let reference = self.object(game, operand)?;
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
    reason = "Counter costs and effects mutate the authoritative object counter map."
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
        let Holder::Object { object } = holder else {
            return Err(unsupported("object counter holder"));
        };
        let reference = execution.object(self, object)?;
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

/// Only a single non-suspending counter instruction is admitted in this loop.
pub(super) fn iteration(
    control: &Control,
) -> Option<(&Selector, Binding<ObjectRef>, &Instruction)> {
    use crate::ir::Node;
    let Control::EachObject {
        input,
        binding,
        body,
        order: None,
    } = control
    else {
        return None;
    };
    let [step] = body.steps.as_slice() else {
        return None;
    };
    let Node::Instruction { instruction } = &step.node else {
        return None;
    };
    if !matches!(&instruction.action, Action::Counter {
        holder: Holder::Object { object: ObjectOperand::Bound { binding: holder } }, ..
    } if holder == binding)
    {
        return None;
    }
    Some((input, *binding, instruction))
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Object iteration delegates to the shared counter instruction."
)]
impl Game {
    pub(super) fn internal_counter_iteration(
        &mut self,
        execution: &mut Execution,
        control: &Control,
    ) -> Result<()> {
        let (input, binding, instruction) =
            iteration(control).ok_or_else(|| unsupported("counter iteration"))?;
        for reference in execution.objects(self, input)? {
            execution.slots.insert(
                binding.key(),
                super::Slot::Objects {
                    members: vec![reference],
                    on_field_at_capture: false,
                },
            );
            self.internal_instruction(execution, Box::new(instruction.clone()))?;
        }
        execution.slots.remove(&binding.key());
        Ok(())
    }
}

pub(super) fn check_iteration(control: &Control) -> Result<()> {
    let (input, _, _) = iteration(control).ok_or_else(|| unsupported("counter iteration"))?;
    check_selector(input)
}

fn check_selector(selector: &Selector) -> Result<()> {
    use crate::ir::ZoneKind;
    match selector {
        Selector::Bound { .. } => Ok(()),
        Selector::Zone {
            zone: ZoneKind::Field | ZoneKind::Ex,
            ..
        } => super::card::selection::public_zone_selector(selector),
        Selector::Difference { input, .. } => {
            super::card::selection::public_zone_selector(selector)?;
            check_selector(input)
        }
        Selector::Union { sets } => {
            for set in sets {
                check_selector(set)?;
            }
            Ok(())
        }
        _ => Err(unsupported("public Field or EX counter selector")),
    }
}
