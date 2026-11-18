//! Continuations preserve each move before the next iteration can overwrite its local receipt.
use super::{
    Control, Execution, Game, InstructionOutcome, MoveOutcome, Node, ObjectRef, ReceiptLocation,
    Result, ScopeId, invalid, seat_name, unsupported,
};
use crate::ir::repeated_moves as ir_repeated_moves;
use crate::ir::{IterationCheck, ReceiptExport, Termination};
use alloc::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub(super) struct State {
    active: BTreeSet<ScopeId>,
    lists: BTreeMap<(ScopeId, u64), Vec<MoveOutcome>>,
    iterations: BTreeMap<(ScopeId, u64), MoveOutcome>,
}

impl State {
    pub(super) fn iteration(&self, key: (ScopeId, u64)) -> Result<&MoveOutcome> {
        self.iterations
            .get(&key)
            .ok_or_else(|| invalid("missing repeat movement"))
    }

    pub(super) fn followed(&self, key: (ScopeId, u64)) -> Result<Vec<ObjectRef>> {
        Ok(self
            .lists
            .get(&key)
            .ok_or_else(|| invalid("missing collected moves"))?
            .iter()
            .flat_map(|outcome| &outcome.moved)
            .filter(|movement| movement.actual_to.is_some())
            .filter_map(|movement| {
                movement.after.filter(|after| {
                    after.id == movement.before.id && after.generation != movement.before.generation
                })
            })
            .collect())
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Repeat continuations share Game's live values and execution queue."
)]
impl Game {
    pub(super) fn internal_repeat_control(
        &self,
        execution: &mut Execution,
        control: Control,
    ) -> Result<()> {
        let Some(plan) = ir_repeated_moves::projection(&control) else {
            return Self::internal_repeat(execution, control);
        };
        let ReceiptExport::Move {
            source,
            iteration,
            collect,
        } = plan.export
        else {
            return Err(unsupported("move repeat export"));
        };
        let ReceiptLocation::Local { id } = source.location() else {
            return Err(unsupported("move repeat producer"));
        };
        let first = execution.repeats.active.insert(plan.scope);
        let progressed = if first {
            if let Some(binding) = collect {
                execution.repeats.lists.insert(binding.key(), vec![]);
            }
            true
        } else {
            let Some(InstructionOutcome::Move(outcome)) = execution.receipts.get(id) else {
                return Err(invalid("missing repeat movement"));
            };
            let moved = outcome.moved.iter().any(|movement| {
                movement.actual_to.is_some()
                    && movement
                        .after
                        .is_some_and(|after| after.generation != movement.before.generation)
            });
            execution
                .repeats
                .iterations
                .insert(iteration.key(), outcome.clone());
            if let Some(binding) = collect {
                execution
                    .repeats
                    .lists
                    .get_mut(&binding.key())
                    .ok_or_else(|| invalid("missing repeat collection"))?
                    .push(outcome.clone());
            }
            moved
        };
        let stop = if first && plan.check == IterationCheck::AfterIteration {
            false
        } else {
            match plan.termination {
                Termination::Until { predicate } => execution.boolean(self, predicate)?.0,
                Termination::While { predicate } => !execution.boolean(self, predicate)?.0,
                Termination::Count { .. } => return Err(unsupported("move repeat termination")),
            }
        };
        let full = if plan.to_ex {
            let capacity =
                super::movement::zone_capacity("ex").ok_or_else(|| invalid("EX capacity"))?;
            self.zone_count(seat_name(execution.context.controller)?, "ex") >= capacity
        } else {
            false
        };
        if stop || !progressed || full {
            execution
                .processing_disclosure
                .procedures
                .finish_repeat(plan.scope);
            execution.repeats.active.remove(&plan.scope);
            execution.repeats.iterations.remove(&iteration.key());
            return Ok(());
        }
        let body = plan.body.clone();
        let scope = plan.scope;
        execution
            .processing_disclosure
            .procedures
            .begin_repeat(scope)?;
        execution.todo.push_front(Node::Control { control }.into());
        execution.todo.push_front(super::Queued::EndRepeat(scope));
        execution.prepend(body);
        Ok(())
    }
}
