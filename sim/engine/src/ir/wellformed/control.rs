use super::{
    BatchOutcome, Binding, BindingKind, Checked, Checker, Control, Environment, InstructionBatch,
    IterationCheck, Program, ReceiptExport, ReceiptList, ReceiptLocation, ReceiptReference,
    RepeatBody, ScopeId, Termination, TypeId, WellFormedReason,
};
use crate::ir::{IterationOrder, StageMode};

impl Checker {
    pub(super) fn control(&mut self, control: &Control) -> Checked {
        match control {
            Control::Sequence { body } => self.isolated(|checker| checker.program(body)),
            Control::If {
                condition,
                then_body,
                else_body,
            } => {
                self.at("condition", |checker| checker.boolean(condition))?;
                self.at("then_body", |checker| {
                    checker.isolated(|checker| checker.program(then_body))
                })?;
                if let Some(body) = else_body {
                    self.at("else_body", |checker| {
                        checker.isolated(|checker| checker.program(body))
                    })?;
                }
                Ok(())
            }
            Control::Simultaneous { batch, receipt } => {
                self.at("batch", |checker| checker.batch(batch))?;
                self.produce(*receipt, TypeId::of::<BatchOutcome>())
            }
            Control::Repeat {
                body: RepeatBody::Program { body },
                termination,
                check,
                scope,
                receipts,
            } => self.repeat(body, termination, *check, *scope, receipts),
            Control::EachObject {
                input,
                binding,
                body,
                order,
            } => {
                if *order == Some(IterationOrder::Chosen) {
                    return self.reject();
                }
                self.selector(input)?;
                self.isolated(|checker| {
                    checker.env.scope = body.scope;
                    checker.define(*binding, BindingKind::Value)?;
                    checker.program(body)
                })
            }
            Control::EachPlayer {
                players,
                player,
                stages,
                ..
            } => {
                self.players(players)?;
                self.isolated(|checker| {
                    let scope = player.key().0;
                    if stages.is_empty() {
                        return checker.reject();
                    }
                    if checker.scopes.contains(&scope) {
                        return Err(checker.error(WellFormedReason::DuplicateScope));
                    }
                    if !stages.iter().any(|stage| stage.body.scope == scope) {
                        checker.scopes.insert(scope);
                    }
                    checker.env.scope = scope;
                    checker.define(*player, BindingKind::Value)?;
                    for (index, stage) in stages.iter().enumerate() {
                        checker.at(&format!("stages/{index}/body"), |checker| {
                            checker.isolated(|checker| {
                                checker.env.choices &=
                                    stage.mode != StageMode::NonChoiceSimultaneous {};
                                checker.program(&stage.body)
                            })
                        })?;
                    }
                    Ok(())
                })
            }
            Control::Match { .. }
            | Control::May { .. }
            | Control::Mode { .. }
            | Control::Repeat { .. }
            | Control::Pay { .. } => self.reject(),
        }
    }

    fn batch(&mut self, batch: &InstructionBatch) -> Checked {
        if batch.members.is_empty() {
            return Err(self.error(WellFormedReason::EmptyBatch));
        }
        if !batch.choice_stages.is_empty() {
            return self.reject();
        }
        let start = self.env.clone();
        let mut committed = start.clone();
        for (index, member) in batch.members.iter().enumerate() {
            self.env = start.clone();
            self.env.choices = false;
            self.at(&format!("members/{index}"), |checker| {
                checker.located(member, Self::instruction)
            })?;
            committed.bindings.extend(self.env.bindings.clone());
            committed.receipts.extend(self.env.receipts.clone());
        }
        self.env = committed;
        Ok(())
    }

    fn repeat(
        &mut self,
        body: &Program,
        termination: &Termination,
        check: IterationCheck,
        scope: ScopeId,
        exports: &[ReceiptExport],
    ) -> Checked {
        if !self.scopes.insert(scope) {
            return Err(self.error(WellFormedReason::DuplicateScope));
        }
        let mut after = self.env.clone();
        let mut iteration = after.clone();
        iteration.scope = scope;
        self.at("body", |checker| checker.program(body))?;
        let produced = self.env.clone();
        for (index, export) in exports.iter().enumerate() {
            self.at(&format!("receipts/{index}"), |checker| {
                checker.export(export, &produced, &mut iteration, &mut after)
            })?;
        }
        self.env = match (termination, check) {
            (Termination::Count { .. }, _) | (_, IterationCheck::BeforeIteration) => {
                let mut before = iteration.clone();
                before
                    .bindings
                    .retain(|_, slot| slot.kind != BindingKind::Iteration);
                before
            }
            (_, IterationCheck::AfterIteration) => iteration,
        };
        self.at("termination", |checker| match termination {
            Termination::Count { count } => checker.natural(count),
            Termination::Until { predicate } | Termination::While { predicate } => {
                checker.boolean(predicate)
            }
        })?;
        self.env = after;
        Ok(())
    }

    fn export(
        &mut self,
        export: &ReceiptExport,
        body: &Environment,
        iteration_env: &mut Environment,
        after: &mut Environment,
    ) -> Checked {
        macro_rules! export {
            ($($variant:ident),* $(,)?) => {
                match export {
                    $(ReceiptExport::$variant { source, iteration, collect } => self.export_typed(source, *iteration, *collect, body, iteration_env, after)),*
                }
            };
        }
        export!(
            Batch,
            Move,
            Search,
            Discard,
            Destroy,
            Banish,
            Order,
            Transform,
            Fusion,
            Token,
            Flip,
            Look,
            CardQuantity,
            DamageQuantity,
            CounterQuantity,
            Payment
        )
    }

    fn export_typed<T>(
        &mut self,
        source: &ReceiptReference<T>,
        iteration: Binding<T>,
        collect: Option<Binding<ReceiptList<T>>>,
        body: &Environment,
        iteration_env: &mut Environment,
        after: &mut Environment,
    ) -> Checked
    where
        T: 'static,
    {
        let ReceiptLocation::Local { id } = source.location() else {
            return Err(self.error(WellFormedReason::InvalidExport));
        };
        if after.receipts.contains_key(id) {
            return Err(self.error(WellFormedReason::InvalidExport));
        }
        self.env = body.clone();
        self.receipt(source)?;
        self.env = iteration_env.clone();
        self.define(iteration, BindingKind::Iteration)?;
        *iteration_env = self.env.clone();
        if let Some(collect) = collect {
            self.env = after.clone();
            self.define(collect, BindingKind::Value)?;
            *after = self.env.clone();
        }
        Ok(())
    }
}
