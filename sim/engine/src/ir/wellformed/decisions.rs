use super::{BindingKind, Checked, Checker, TypeId, WellFormedReason};
use crate::ir::AggregateBound;
use crate::ir::{
    Action, CaptureTime, Cardinality, ChoiceDomain, CostSpec, CounterOperation, Decision,
    DecisionKind, Declinable, DiscardMode, Disjointness, ExecutionRef, InputMode,
    InstructionContext, MemberProjection, MoveDestination, MoveReason, Nat, Node, ObjectOperand,
    OwnerZone, PaymentOptionality, PaymentResult, PaymentTime, PlaySlot, PlayerRef, PlayerRelation,
    Posture, Program, ReceiptId, RoleCardinality, RuleProcedure, SelectionConstraint,
    SelectionView, Selector, TargetRoleSpec, Visibility, ZoneKind, ZoneRef,
};
use crate::ir::{Expr, Int, NumericChange, ObjectReceipt, ObjectReceiptList, ResourceOperation};
use alloc::collections::BTreeSet;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Decision and payment rules share the structural checker state."
)]
impl Checker {
    pub(super) fn program_steps(&mut self, program: &Program) -> Checked {
        let mut steps = program.steps.iter().enumerate();
        while let Some((index, step)) = steps.next() {
            let guard = match &step.node {
                Node::Decision {
                    decision:
                        Decision {
                            kind: DecisionKind::ResolutionChoice { precondition, .. },
                            ..
                        },
                } => precondition.as_ref(),
                Node::Decision { .. }
                | Node::Instruction { .. }
                | Node::Control { .. }
                | Node::Capture { .. } => None,
            };
            let bindings = self.env.bindings.clone();
            let access = self.env.access.clone();
            self.at(&index.to_string(), |checker| {
                checker.located(step, Self::node)
            })?;
            if let Some(guard) = guard {
                let Some((next_index, next)) = steps.next() else {
                    return self.at(&index.to_string(), |checker| {
                        checker.located(step, |checker, _| {
                            checker.at("decision/precondition", |checker| {
                                Err(checker.error(WellFormedReason::PreconditionTiming))
                            })
                        })
                    });
                };
                self.at(&next_index.to_string(), |checker| {
                    checker.located(next, |checker, node| {
                        let Node::Instruction { instruction } = node else {
                            return Err(checker.error(WellFormedReason::PreconditionTiming));
                        };
                        if instruction.precondition.as_ref() != Some(guard)
                            || guard.protects != instruction.id
                        {
                            return Err(checker.error(WellFormedReason::PreconditionTiming));
                        }
                        checker.guarded_instruction(instruction)
                    })
                })?;
                // A skipped instruction never supplies its guarded selection to later nodes.
                self.env.bindings = bindings;
                self.env.access = access;
                self.env
                    .distributions
                    .retain(|key, _| self.env.bindings.contains_key(key));
            }
        }
        Ok(())
    }

    pub(super) fn decision(&mut self, decision: &Decision) -> Checked {
        let DecisionKind::ResolutionChoice {
            may,
            cardinality,
            precondition,
        } = &decision.kind
        else {
            return Err(self.error(WellFormedReason::DecisionTiming));
        };
        if *may != Declinable::Required {
            return self.reject();
        }
        if let Some(guard) = precondition {
            self.at("precondition", |checker| checker.boolean(&guard.predicate))?;
        }
        self.decision_header(decision)?;
        self.answer(decision, Some(*cardinality))
    }

    pub(super) fn decision_header(&mut self, decision: &Decision) -> Checked {
        if !self.env.choices {
            return self.reject();
        }
        if !self.decisions.insert(decision.id) {
            return Err(self.error(WellFormedReason::DuplicateDecision));
        }
        self.player(&decision.actor)?;
        self.visibility(&decision.visibility)
    }

    pub(super) fn answer(
        &mut self,
        decision: &Decision,
        cardinality: Option<Cardinality>,
    ) -> Checked {
        if let Some(cardinality) = cardinality {
            self.cardinality(cardinality)?;
        }
        if has_roles(&decision.domain)
            && !matches!(
                decision.kind,
                DecisionKind::PlayParameter {
                    slot: PlaySlot::Targets,
                    ..
                }
            )
        {
            return self.reject();
        }
        self.env.chooser = Some(decision.actor.clone());
        self.env.private_choice =
            super::visibility::private_to(&decision.visibility, &decision.actor);
        let result = self.at("domain", |checker| {
            checker.domain(&decision.domain, cardinality)
        });
        self.env.chooser = None;
        self.env.private_choice = false;
        result?;
        self.answer_access(&decision.domain, &decision.visibility);
        self.at("captures", |checker| {
            checker.captures(
                &decision.captures,
                CaptureTime::AtSelection {
                    decision: decision.id,
                },
            )
        })
    }

    pub(super) fn visibility(&mut self, visibility: &Visibility) -> Checked {
        match visibility {
            Visibility::Public {} | Visibility::AuthorityOnly {} => Ok(()),
            Visibility::Only { players } => self.players(players),
        }
    }

    fn domain(&mut self, domain: &ChoiceDomain, cardinality: Option<Cardinality>) -> Checked {
        match domain {
            ChoiceDomain::Objects {
                constraint,
                binding,
                target_roles,
            } => {
                if cardinality.is_some_and(|cardinality| constraint.cardinality != cardinality) {
                    return Err(self.error(WellFormedReason::DecisionCardinality));
                }
                self.constraint(constraint)?;
                self.define(*binding, BindingKind::Value)?;
                if let Some(roles) = target_roles {
                    self.roles(roles, constraint.cardinality)?;
                }
                Ok(())
            }
            ChoiceDomain::Recipients {
                candidates,
                cardinality: bounds,
                binding,
            } => {
                self.cardinality(*bounds)?;
                if cardinality.is_some_and(|value| value != *bounds) {
                    return Err(self.error(WellFormedReason::DecisionCardinality));
                }
                self.recipients(candidates)?;
                self.define(*binding, BindingKind::Value)
            }
            ChoiceDomain::Integer {
                cardinality: bounds,
                binding,
            } => {
                self.cardinality(*bounds)?;
                if cardinality.is_some_and(|cardinality| *bounds != cardinality) {
                    return Err(self.error(WellFormedReason::DecisionCardinality));
                }
                self.define(*binding, BindingKind::Value)
            }
            ChoiceDomain::Compound { domains } => {
                // The envelope cardinality counts execution, not members of heterogeneous payloads.
                if cardinality.is_some_and(|value| {
                    value
                        != (Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        })
                }) {
                    return Err(self.error(WellFormedReason::DecisionCardinality));
                }
                let start = self.env.clone();
                let mut after = start.clone();
                for (index, child) in domains.iter().enumerate() {
                    self.env = start.clone();
                    self.at(&index.to_string(), |checker| checker.domain(child, None))?;
                    after.bindings.extend(self.env.bindings.clone());
                    after.distributions.extend(self.env.distributions.clone());
                }
                self.env = after;
                Ok(())
            }
            ChoiceDomain::Distribution {
                recipients,
                total,
                binding,
                ..
            } => {
                if cardinality.is_some_and(|value| {
                    value
                        != (Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        })
                }) {
                    return Err(self.error(WellFormedReason::DecisionCardinality));
                }
                self.recipients(recipients)?;
                self.natural(total)?;
                self.define(*binding, BindingKind::Value)?;
                self.env
                    .distributions
                    .insert(binding.key(), recipients.clone());
                Ok(())
            }
            ChoiceDomain::EndPhaseGuardAct { .. }
            | ChoiceDomain::Modes { .. }
            | ChoiceDomain::Order { .. }
            | ChoiceDomain::Names { .. }
            | ChoiceDomain::QuickActionOrPass { .. } => self.reject(),
        }
    }

    pub(super) fn play_decision(&mut self, decision: &Decision, controller: &PlayerRef) -> Checked {
        let DecisionKind::PlayParameter {
            slot,
            play_scope,
            input,
            ..
        } = decision.kind
        else {
            return Err(self.error(WellFormedReason::DecisionTiming));
        };
        self.play_input(play_scope, input, &decision.actor, controller)?;
        if decision.visibility != (Visibility::Public {}) {
            return self.reject();
        }
        match (slot, &decision.domain) {
            (PlaySlot::CostMaterials, ChoiceDomain::Objects { .. })
            | (PlaySlot::Targets, ChoiceDomain::Objects { .. } | ChoiceDomain::Recipients { .. })
            | (PlaySlot::X, ChoiceDomain::Integer { .. })
            | (PlaySlot::Distribution, ChoiceDomain::Distribution { .. }) => {}
            _ => return self.reject(),
        }
        self.decision_header(decision)?;
        self.answer(decision, None)
    }

    pub(super) fn play_input(
        &mut self,
        execution: ExecutionRef,
        input: InputMode,
        actor: &PlayerRef,
        controller: &PlayerRef,
    ) -> Checked {
        if execution != (ExecutionRef::Current {}) {
            return Err(self.error(WellFormedReason::DecisionTiming));
        }
        let valid = match input {
            InputMode::Inline => actor == controller,
            InputMode::Handoff => matches!(actor, PlayerRef::RequireOne { players }
                if players.relation == PlayerRelation::Opponents && &players.relative_to == controller
                    && players.cardinality == (Cardinality { minimum: Nat(1), maximum: Some(Nat(1)) })),
        };
        if !valid {
            return Err(self.error(WellFormedReason::DecisionTiming));
        }
        self.player(actor)
    }

    pub(super) fn rule_decision(
        &mut self,
        procedure: RuleProcedure,
        decision: &Decision,
    ) -> Checked {
        let ChoiceDomain::EndPhaseGuardAct { binding } = &decision.domain else {
            return self.reject();
        };
        if procedure != RuleProcedure::GuardAct
            || decision.visibility != (Visibility::Public {})
            || decision.actor != (PlayerRef::Active {})
            || !decision.captures.is_empty()
        {
            return self.reject();
        }
        self.decision_header(decision)?;
        self.define(*binding, BindingKind::Value)
    }

    fn roles(&mut self, spec: &TargetRoleSpec, selected: Cardinality) -> Checked {
        if spec.roles.is_empty() || spec.roles.len() > 2 {
            return Err(self.error(WellFormedReason::InvalidRoles));
        }
        let mut ids = BTreeSet::new();
        let mut first = false;
        let mut remainder = false;
        for role in &spec.roles {
            if !ids.insert(role.role.0) {
                return Err(self.error(WellFormedReason::InvalidRoles));
            }
            match role.cardinality {
                RoleCardinality::IfNonempty { count: Nat(1) } if !first => first = true,
                RoleCardinality::Remainder {} if !remainder => remainder = true,
                RoleCardinality::IfNonempty { .. }
                | RoleCardinality::Remainder {}
                | RoleCardinality::Fixed { .. } => {
                    return Err(self.error(WellFormedReason::InvalidRoles));
                }
            }
            self.define(role.binding, BindingKind::Value)?;
        }
        if !first || (!remainder && selected.maximum.is_none_or(|maximum| maximum.0 > 1)) {
            return Err(self.error(WellFormedReason::InvalidRoles));
        }
        Ok(())
    }

    fn constraint(&mut self, constraint: &SelectionConstraint) -> Checked {
        self.cardinality(constraint.cardinality)?;
        if constraint.groups.is_empty() {
            return self.reject();
        }
        for (index, group) in constraint.groups.iter().enumerate() {
            self.at(&format!("groups/{index}"), |checker| {
                checker.cardinality(group.cardinality)?;
                checker.selector(&group.selector)
            })?;
        }
        self.aggregate_bounds(&constraint.aggregate_bounds)
    }

    pub(super) fn aggregate_bounds(&mut self, bounds: &[AggregateBound]) -> Checked {
        for (index, bound) in bounds.iter().enumerate() {
            self.at(&format!("aggregate_bounds/{index}"), |checker| {
                checker.integer(&bound.value)
            })?;
        }
        Ok(())
    }

    pub(super) fn discard(&mut self, player: &PlayerRef, mode: &DiscardMode) -> Checked {
        self.player(player)?;
        self.env.chooser = Some(player.clone());
        self.env.private_choice = true;
        let result = self.discard_mode(player, mode);
        self.env.chooser = None;
        self.env.private_choice = false;
        result
    }

    fn discard_mode(&mut self, player: &PlayerRef, mode: &DiscardMode) -> Checked {
        match mode {
            DiscardMode::Chosen { constraint } => {
                if !self.env.choices {
                    return self.reject();
                }
                if constraint.disjoint != Disjointness::Disjoint {
                    return self.reject();
                }
                self.constraint(constraint)?;
                for group in &constraint.groups {
                    self.hand(&group.selector, player)?;
                }
                Ok(())
            }
            DiscardMode::Random { population, count } => {
                self.selector(population)?;
                self.hand(population, player)?;
                self.integer(count)
            }
            DiscardMode::All { eligible } => {
                self.selector(eligible)?;
                self.hand(eligible, player)
            }
            DiscardMode::DownTo { .. } => {
                if !self.env.choices {
                    return self.reject();
                }
                Ok(())
            }
        }
    }

    pub(super) fn selection_visibility(&self, selector: &Selector) -> Checked {
        let Some(actor) = &self.env.chooser else {
            return Ok(());
        };
        match selector {
            Selector::SearchFound { found } => {
                self.visible_object(self.env.access.get(&found.key()))
            }
            Selector::SearchRevealed { revealed } => {
                self.visible_object(self.env.access.get(&revealed.key()))
            }
            Selector::Zone {
                players,
                zone: ZoneKind::Hand,
                ..
            } if !self.env.private_choice
                || players.relation != PlayerRelation::SelfPlayer
                || players.relative_to != *actor =>
            {
                self.reject()
            }
            Selector::Zone {
                zone: ZoneKind::Deck | ZoneKind::Evolve | ZoneKind::Banish,
                ..
            }
            | Selector::Ordered { .. } => self.reject(),
            Selector::CollectedMembers { receipts, .. } => match receipts {
                ObjectReceiptList::Look { receipts } => {
                    self.visible_object(self.env.access.get(&receipts.key()))
                }
                ObjectReceiptList::Token { receipts } => {
                    self.visible_object(self.env.access.get(&receipts.key()))
                }
                ObjectReceiptList::Flip { .. } | ObjectReceiptList::Batch { .. } => self.reject(),
            },
            Selector::Bound { reference } => {
                self.visible_object(self.env.access.get(&reference.key()))
            }
            Selector::Captured { reference } => {
                self.visible_object(self.env.access.get(&reference.binding.key()))
            }
            Selector::Created { receipt } => self.visible_receipt(receipt),
            Selector::ReceiptMembers {
                receipt,
                projection,
            } => match receipt {
                ObjectReceipt::Look { receipt } => self.visible_receipt(receipt),
                ObjectReceipt::Token { receipt } => self.visible_receipt(receipt),
                ObjectReceipt::Movement { receipt } if *projection == MemberProjection::Before => {
                    self.movement_before_visible(receipt)
                }
                ObjectReceipt::Movement { receipt } => self.movement_visible(receipt),
                ObjectReceipt::Batch { .. }
                | ObjectReceipt::TransformBanished { .. }
                | ObjectReceipt::TransformCreated { .. }
                | ObjectReceipt::Flip { .. } => self.reject(),
            },
            Selector::MoveAfterAll { receipts, .. } => self.movements_visible(*receipts),
            Selector::MoveAfter { receipt, .. } => self.movement_visible(receipt),
            Selector::Zone {
                zone:
                    ZoneKind::Hand
                    | ZoneKind::Field
                    | ZoneKind::Ex
                    | ZoneKind::Cemetery
                    | ZoneKind::Resolution
                    | ZoneKind::Race
                    | ZoneKind::Drive
                    | ZoneKind::Trigger
                    | ZoneKind::Equipment,
                ..
            }
            | Selector::Filter { .. }
            | Selector::Union { .. }
            | Selector::Intersection { .. }
            | Selector::Difference { .. }
            | Selector::Objects { .. }
            | Selector::Relation { .. } => Ok(()),
        }
    }

    fn hand(&self, selector: &Selector, player: &PlayerRef) -> Checked {
        match selector {
            Selector::Zone {
                players,
                zone,
                view: SelectionView::Current {},
                ..
            } if *zone == ZoneKind::Hand
                && players.relation == PlayerRelation::SelfPlayer
                && players.relative_to == *player =>
            {
                Ok(())
            }
            Selector::Zone { .. } => Err(self.error(WellFormedReason::InvalidDiscardDomain)),
            Selector::Filter { input, .. } | Selector::Difference { input, .. } => {
                self.hand(input, player)
            }
            Selector::Union { sets } => {
                for set in sets {
                    self.hand(set, player)?;
                }
                Ok(())
            }
            Selector::Intersection { sets }
                if sets.iter().any(|set| self.hand(set, player).is_ok()) =>
            {
                Ok(())
            }
            Selector::Objects { objects } if objects.is_empty() => Ok(()),
            Selector::CollectedMembers { .. }
            | Selector::MoveAfterAll { .. }
            | Selector::Created { .. }
            | Selector::ReceiptMembers { .. }
            | Selector::MoveAfter { .. }
            | Selector::Intersection { .. }
            | Selector::Ordered { .. }
            | Selector::SearchFound { .. }
            | Selector::SearchRevealed { .. }
            | Selector::Objects { .. }
            | Selector::Captured { .. }
            | Selector::Bound { .. }
            | Selector::Relation { .. } => self.reject(),
        }
    }

    pub(super) fn payment(
        &mut self,
        at: PaymentTime,
        costs: &CostSpec,
        optional: PaymentOptionality,
        on_paid: &Program,
        receipt: ReceiptId,
    ) -> Checked {
        if at == PaymentTime::PlayParameter
            && (self.env.play_payments != Some(self.root)
                || self.env.scope != self.root
                || !on_paid.steps.is_empty())
        {
            return Err(self.error(WellFormedReason::DecisionTiming));
        }
        if !self.env.choices && optional == PaymentOptionality::Optional {
            return self.reject();
        }
        self.isolated(|checker| {
            checker.env.scope = on_paid.scope;
            checker.env.play_payments = None;
            checker.at("costs", |checker| checker.costs(costs))?;
            checker.produce(receipt, TypeId::of::<PaymentResult>())?;
            checker.at("on_paid", |checker| checker.program(on_paid))
        })?;
        self.env
            .receipts
            .insert(receipt, TypeId::of::<PaymentResult>());
        Ok(())
    }

    pub(super) fn costs(&mut self, costs: &CostSpec) -> Checked {
        match costs {
            CostSpec::Alternative { alternatives } => {
                if !self.env.choices || alternatives.is_empty() {
                    return self.reject();
                }
                for (index, alternative) in alternatives.iter().enumerate() {
                    self.at(&format!("alternatives/{index}"), |checker| {
                        checker.isolated(|checker| checker.costs(alternative))
                    })?;
                }
            }
            CostSpec::Ordered { steps } => {
                for (index, step) in steps.iter().enumerate() {
                    self.at(&index.to_string(), |checker| {
                        if step.instruction.context != InstructionContext::Cost {
                            return Err(checker.error(WellFormedReason::ExpectedCost));
                        }
                        if step.instruction.precondition.is_some() {
                            return Err(checker.error(WellFormedReason::PreconditionTiming));
                        }
                        let valid = matches!(&step.instruction.action,
                            Action::Discard { .. }
                            | Action::Counter { operation: CounterOperation::Remove {}, .. }
                            | Action::Posture { posture: Posture::Acted, .. })
                            || matches!(&step.instruction.action, Action::Resource { operation: ResourceOperation::Pay, amount: Expr::Literal(Int(value)), .. } if *value >= 0)
                            || matches!(&step.instruction.action, Action::Life { change: NumericChange::Add { amount: Expr::Literal(Int(value)) }, .. } if *value <= 0);
                        let source_move = matches!(&step.instruction.action,
                            Action::Move { objects: Selector::Objects { objects }, movement, reason: MoveReason::Put }
                            if objects == &[ObjectOperand::SourceHolder {}]
                                && movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery })
                                && movement.position.is_none() && movement.order.is_none()
                                && movement.initial_posture.is_none() && movement.entry_effects.is_empty());
                        let selected_move = matches!(&step.instruction.action,
                            Action::Move { objects: Selector::Bound { .. }, movement, reason: MoveReason::Put }
                                if (matches!(movement.to, MoveDestination::Owners { zone: OwnerZone::Cemetery | OwnerZone::Hand })
                                    || matches!(movement.to, MoveDestination::Shared { zone: ZoneRef { player: PlayerRef::Controller {}, kind: ZoneKind::Ex } }))
                                    && movement.position.is_none() && movement.order.is_none()
                                    && movement.initial_posture.is_none() && movement.entry_effects.is_empty())
                            || matches!(&step.instruction.action, Action::Banish { objects: Selector::Bound { .. } });
                        if !valid && !source_move && !selected_move {
                            return checker.reject();
                        }
                        checker.instruction_in(
                            &step.instruction,
                            InstructionContext::Cost,
                            false,
                        )?;
                        if let Some(materials) = step.materials {
                            if !matches!(
                                step.instruction.action,
                                Action::Discard { .. } | Action::Posture { .. }
                            ) {
                                return checker.reject();
                            }
                            checker.define(materials, BindingKind::Value)?;
                        }
                        Ok(())
                    })?;
                }
            }
        }
        Ok(())
    }
}

fn has_roles(domain: &ChoiceDomain) -> bool {
    match domain {
        ChoiceDomain::Objects { target_roles, .. } => target_roles.is_some(),
        ChoiceDomain::Compound { domains } => domains.iter().any(has_roles),
        ChoiceDomain::Recipients { .. }
        | ChoiceDomain::EndPhaseGuardAct { .. }
        | ChoiceDomain::Modes { .. }
        | ChoiceDomain::Order { .. }
        | ChoiceDomain::Distribution { .. }
        | ChoiceDomain::Integer { .. }
        | ChoiceDomain::Names { .. }
        | ChoiceDomain::QuickActionOrPass { .. } => false,
    }
}
