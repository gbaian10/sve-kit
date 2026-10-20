use super::{
    BatchOutcome, Binding, BindingKind, Checked, Checker, Control, Environment, InstructionBatch,
    IterationCheck, Program, ReceiptExport, ReceiptList, ReceiptLocation, ReceiptReference,
    RepeatBody, ScopeId, Termination, TypeId, WellFormedReason,
};
use crate::ir::{
    Decision, DecisionKind, Declinable, IterationOrder, PlayerRef, PlayerStage, ReceiptId,
    StageMode,
};
use alloc::collections::BTreeMap;

impl Checker {
    pub(super) fn control(&mut self, control: &Control) -> Checked {
        let modes = self.env.play_modes;
        if !matches!(control, Control::Sequence { .. } | Control::Mode { .. }) {
            self.env.play_modes = false;
        }
        let result = self.control_inner(control);
        self.env.play_modes = modes;
        result
    }

    fn control_inner(&mut self, control: &Control) -> Checked {
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
                let access = self.infer_access(|checker| checker.selector(input));
                self.selector(input)?;
                self.revoke_before_loop(body);
                self.isolated(|checker| {
                    checker.env.scope = body.scope;
                    checker.define(*binding, BindingKind::Value)?;
                    checker.save_access(binding.key(), access);
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
                self.isolated(|checker| checker.each_player(*player, stages))
            }
            Control::Pay {
                at,
                costs,
                optional,
                on_paid,
                receipt,
            } => self.payment(*at, costs, *optional, on_paid, *receipt),
            Control::May {
                body,
                choice,
                receipt,
            } => self.optional(body, choice, *receipt),
            Control::Mode {
                modes,
                count,
                chosen,
                choice_kind,
            } => self.modes(modes, count, chosen, *choice_kind),
            Control::Match { .. } | Control::Repeat { .. } => self.reject(),
        }
    }

    fn each_player(&mut self, player: Binding<PlayerRef>, stages: &[PlayerStage]) -> Checked {
        let scope = player.key().0;
        if stages.is_empty() {
            return self.reject();
        }
        if self.scopes.contains(&scope) {
            return Err(self.error(WellFormedReason::DuplicateScope));
        }
        if !stages.iter().any(|stage| stage.body.scope == scope) {
            self.scopes.insert(scope);
        }
        self.env.scope = scope;
        self.define(player, BindingKind::Value)?;
        for (index, stage) in stages.iter().enumerate() {
            // One player's flip precedes the next player's turn at this stage.
            self.revoke_before_loop(&stage.body);
            self.at(&format!("stages/{index}/body"), |checker| {
                let choices = checker.env.choices;
                checker.env.choices &= stage.mode != StageMode::NonChoiceSimultaneous {};
                checker.program(&stage.body)?;
                checker.env.choices = choices;
                Ok(())
            })?;
        }
        Ok(())
    }

    fn optional(&mut self, body: &Program, choice: &Decision, receipt: ReceiptId) -> Checked {
        let DecisionKind::ResolutionChoice {
            may: Declinable::MayDecline,
            cardinality,
            precondition: None,
        } = choice.kind
        else {
            return Err(self.error(WellFormedReason::DecisionTiming));
        };
        self.isolated(|checker| {
            checker.env.scope = body.scope;
            checker.at("choice", |checker| {
                checker.decision_header(choice)?;
                checker.answer(choice, Some(cardinality))
            })?;
            checker.env.choices = false;
            checker.at("body", |checker| checker.program(body))
        })?;
        self.produce(receipt, TypeId::of::<super::OpaqueOutcome>())
    }

    fn batch(&mut self, batch: &InstructionBatch) -> Checked {
        if batch.members.is_empty() {
            return Err(self.error(WellFormedReason::EmptyBatch));
        }
        let entry = self.env.clone();
        let mut committed = entry.clone();
        let mut guarded = BTreeMap::<_, Environment>::new();
        self.batch_guards(batch, &entry)?;
        self.env = entry;
        for (index, stage) in batch.choice_stages.iter().enumerate() {
            let start = self.env.clone();
            let mut after = start.clone();
            for (decision_index, decision) in stage.decisions.iter().enumerate() {
                self.env = start.clone();
                self.at(
                    &format!("choice_stages/{index}/decisions/{decision_index}"),
                    |checker| checker.batch_decision(batch, decision),
                )?;
                if let DecisionKind::ResolutionChoice {
                    precondition: Some(guard),
                    ..
                } = &decision.kind
                {
                    let target = guarded
                        .entry(guard.protects)
                        .or_insert_with(|| start.clone());
                    target.bindings.extend(self.env.bindings.clone());
                    target.access.extend(self.env.access.clone());
                    target.distributions.extend(self.env.distributions.clone());
                } else {
                    after.bindings.extend(self.env.bindings.clone());
                    after.access.extend(self.env.access.clone());
                    after.distributions.extend(self.env.distributions.clone());
                }
            }
            self.env = after;
        }
        let start = self.env.clone();
        for (index, member) in batch.members.iter().enumerate() {
            self.env = start.clone();
            if let Some(answers) = guarded.get(&member.node.id) {
                self.env.bindings.extend(answers.bindings.clone());
                self.env.distributions.extend(answers.distributions.clone());
                self.env.access.extend(answers.access.clone());
            }
            let inputs = self.env.bindings.clone();
            self.env.choices = false;
            self.at(&format!("members/{index}"), |checker| {
                checker.located(member, Self::guarded_instruction)
            })?;
            committed.bindings.extend(
                self.env
                    .bindings
                    .iter()
                    .filter(|(key, _)| !inputs.contains_key(key))
                    .map(|(key, slot)| (*key, *slot)),
            );
            committed.receipts.extend(self.env.receipts.clone());
            committed
                .receipt_access
                .extend(self.env.receipt_access.clone());
            committed.access.extend(
                self.env
                    .access
                    .iter()
                    .filter(|(key, _)| !inputs.contains_key(key))
                    .map(|(key, access)| (*key, access.clone())),
            );
        }
        self.env = committed;
        Ok(())
    }

    fn batch_decision(&mut self, batch: &InstructionBatch, decision: &Decision) -> Checked {
        if let DecisionKind::ResolutionChoice {
            precondition: Some(guard),
            ..
        } = &decision.kind
        {
            let mut members = batch
                .members
                .iter()
                .filter(|member| member.node.id == guard.protects);
            let Some(member) = members.next() else {
                return Err(self.error(WellFormedReason::PreconditionTiming));
            };
            if members.next().is_some() || member.node.precondition.as_ref() != Some(guard) {
                return Err(self.error(WellFormedReason::PreconditionTiming));
            }
            let mut unguarded = decision.clone();
            if let DecisionKind::ResolutionChoice { precondition, .. } = &mut unguarded.kind {
                *precondition = None;
            }
            self.decision(&unguarded)
        } else {
            self.decision(decision)
        }
    }

    fn batch_guards(&mut self, batch: &InstructionBatch, entry: &Environment) -> Checked {
        for (index, member) in batch.members.iter().enumerate() {
            if let Some(guard) = &member.node.precondition {
                self.env = entry.clone();
                self.at(&format!("members/{index}/precondition"), |checker| {
                    checker.located(member, |checker, instruction| {
                        if guard.protects != instruction.id {
                            return Err(checker.error(WellFormedReason::PreconditionTiming));
                        }
                        checker.boolean(&guard.predicate)
                    })
                })?;
            }
        }
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
        self.revoke_before_loop(body);
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
        let access = body.receipt_access.get(id).cloned();
        self.env = body.clone();
        self.receipt(source)?;
        self.env = iteration_env.clone();
        self.define(iteration, BindingKind::Iteration)?;
        self.save_access(iteration.key(), access.clone());
        *iteration_env = self.env.clone();
        if let Some(collect) = collect {
            self.env = after.clone();
            self.define(collect, BindingKind::Value)?;
            self.save_access(collect.key(), access);
            *after = self.env.clone();
        }
        Ok(())
    }
}
