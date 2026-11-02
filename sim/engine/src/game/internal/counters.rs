//! Card counters share the object's authoritative counter map.

#![expect(
    clippy::indexing_slicing,
    reason = "Counter writes address a constructed object state map."
)]

use super::{Execution, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Action, Binding, ChoiceDomain, Control, CounterChangeEvent, CounterKind, CounterOperation,
    Expr, Holder, Instruction, Int, IntOperation, Nat, NatRead, ObjectOperand, ObjectRef, PlaySpec,
    PlayerRef, PlayerRelation, Quantity, QuantityUnit, RuntimeHolder, Selector, ZoneKind,
};
use serde_json::json;

const fn object_holder(holder: &Holder) -> bool {
    matches!(
        holder,
        Holder::Object {
            object: ObjectOperand::SourceHolder {}
                | ObjectOperand::Bound { .. }
                | ObjectOperand::Selected { .. }
        }
    )
}

pub(super) fn check_action(action: &Action) -> Result<()> {
    let Action::Counter {
        holder,
        kind,
        operation,
        amount,
    } = action
    else {
        return Ok(());
    };
    let fixed = matches!(amount, Expr::Literal(Nat(n)) if i64::try_from(*n).is_ok());
    let valid = match operation {
        CounterOperation::Add {} | CounterOperation::Remove {} => fixed,
        CounterOperation::Move { to } => {
            object_holder(to)
                && (fixed
                    || matches!(
                        amount, Expr::Live(read) if matches!(read.as_ref(), NatRead::Counter { holder: source, kind: source_kind } if source == holder && source_kind == kind)
                    ))
        }
    };
    if !object_holder(holder) || !valid {
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
                object: ObjectOperand::SourceHolder {} | ObjectOperand::Selected { .. }
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
            return Err(unsupported("object counter read"));
        };
        if !matches!(
            operand,
            ObjectOperand::SourceHolder {}
                | ObjectOperand::Bound { .. }
                | ObjectOperand::Selected { .. }
        ) {
            return Err(unsupported("counter read holder"));
        }
        let reference = self.object(game, operand)?;
        let id = self.object_id(reference)?;
        if game.object(id)?.generation != reference.generation.0 {
            return Err(unsupported("stale counter holder requires a snapshot"));
        }
        game.card_counter(id, kind)
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Selection predicates and instructions share the authoritative counter read."
)]
impl Game {
    pub(super) fn card_counter(&self, id: &str, kind: &CounterKind) -> Result<Nat> {
        let object = self.object(id)?;
        let key = self.catalog.keyword_id(&kind.0);
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
    fn internal_counter_move(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        holder: &Holder,
        kind: &CounterKind,
        destination: &Holder,
        amount: &Expr<Nat>,
    ) -> Result<Quantity> {
        let before = execution.counter(self, holder, kind)?;
        let recipient_before = execution.counter(self, destination, kind)?;
        let Holder::Object { object: source } = holder else {
            return Err(unsupported("counter source"));
        };
        let Holder::Object { object: recipient } = destination else {
            return Err(unsupported("counter destination"));
        };
        let from = execution.object(self, source)?;
        let to = execution.object(self, recipient)?;
        for reference in [from, to] {
            if !matches!(
                self.object(execution.object_id(reference)?)?.zone.as_str(),
                "field" | "ex"
            ) {
                return Err(unsupported("counter holder must be in Field or EX"));
            }
        }
        let moved = if from == to {
            0
        } else {
            execution.natural(self, amount)?.0.min(before.0)
        };
        let recipient_after = recipient_before
            .0
            .checked_add(moved)
            .filter(|value| i64::try_from(*value).is_ok())
            .ok_or_else(|| invalid("counter overflow"))?;
        let key = self.catalog.keyword_id(&kind.0);
        let removed = counter_change(from, &key, before, Nat(before.0.saturating_sub(moved)))?;
        let added = counter_change(to, &key, recipient_before, Nat(recipient_after))?;
        // Validate both endpoints before committing either counter change.
        let group = self.group();
        self.commit_counter_change(execution, &removed, group)?;
        self.commit_counter_change(execution, &added, group)?;
        execution.receipts.insert(
            instruction.receipt,
            InstructionOutcome::CounterMove {
                from: removed,
                to: added,
            },
        );
        Ok(Quantity {
            amount: Int(i64::try_from(moved).map_err(invalid)?),
            unit: QuantityUnit::Counters,
        })
    }

    fn commit_counter_change(
        &mut self,
        execution: &Execution,
        change: &CounterChangeEvent,
        group: u64,
    ) -> Result<()> {
        let RuntimeHolder::Object { object } = change.holder else {
            return Err(unsupported("object counter change"));
        };
        if change.delta.0 == 0 {
            return Ok(());
        }
        let id = execution.object_id(object)?.to_owned();
        self.object_mut(&id)?.state["counters"][&change.kind.0] = json!(change.after.0);
        let translated = self.catalog.keyword_name(&change.kind.0);
        let name = if translated.is_empty() {
            change.kind.0.as_str()
        } else {
            translated
        };
        self.emit(
            json!({"kind":"カウンター","object":id,"name":name,"delta":change.delta.0}),
            &execution.frame().cause,
            group,
        );
        Ok(())
    }

    pub(super) fn internal_counter(
        &mut self,
        execution: &mut Execution,
        instruction: &Instruction,
        payment: bool,
    ) -> Result<Quantity> {
        check_action(&instruction.action)?;
        if let Action::Counter {
            holder,
            kind,
            operation: CounterOperation::Move { to },
            amount,
        } = &instruction.action
        {
            if payment {
                return Err(unsupported("counter movement is not a payment"));
            }
            return self.internal_counter_move(execution, instruction, holder, kind, to, amount);
        }
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

pub(super) fn selected_cost(action: &Action) -> Option<Binding<Selector>> {
    match action {
        Action::Counter {
            holder:
                Holder::Object {
                    object: ObjectOperand::Selected { selection },
                },
            operation: CounterOperation::Remove {},
            ..
        } if check_action(action).is_ok() => Some(*selection),
        _ => None,
    }
}

pub(super) fn check_cost(action: &Action, play: &PlaySpec) -> bool {
    if !is_cost(action) {
        return false;
    }
    let Some(selected) = selected_cost(action) else {
        return true;
    };
    play.parameters.iter().any(|decision| {
        super::card::material::check_parameter(decision).is_ok()
            && matches!(&decision.domain, ChoiceDomain::Objects { binding, constraint, .. }
                if *binding == selected && constraint.cardinality.minimum == Nat(1)
                    && constraint.groups.first().is_some_and(|group| own_pool(&group.selector)))
    })
}

fn own_pool(selector: &Selector) -> bool {
    match selector {
        Selector::Zone {
            players,
            zone: ZoneKind::Field | ZoneKind::Ex,
            ..
        } => {
            players.relation == PlayerRelation::SelfPlayer
                && players.relative_to == (PlayerRef::Controller {})
        }
        Selector::Difference { input, .. } => own_pool(input),
        _ => false,
    }
}

pub(super) fn candidate_read(read: &IntOperation, candidate: Binding<ObjectRef>) -> bool {
    matches!(read, IntOperation::FromNat { value: Expr::Live(value) }
        if matches!(value.as_ref(), NatRead::Counter {
            holder: Holder::Object { object: ObjectOperand::Bound { binding } }, kind
        } if *binding == candidate && !kind.0.is_empty()))
}

fn counter_change(
    reference: ObjectRef,
    kind: &str,
    before: Nat,
    after: Nat,
) -> Result<CounterChangeEvent> {
    let delta = i64::try_from(after.0)
        .map_err(invalid)?
        .checked_sub(i64::try_from(before.0).map_err(invalid)?)
        .ok_or_else(|| invalid("counter delta overflow"))?;
    Ok(CounterChangeEvent {
        holder: RuntimeHolder::Object { object: reference },
        kind: CounterKind(kind.into()),
        before,
        after,
        delta: Int(delta),
    })
}
