use super::{BindingKind, Checked, Checker, TypeId, WellFormedReason};
use crate::ir::TurnOperation;
use crate::ir::observation_selection::{private_look_choice, revealed_choice};
use crate::ir::reveal_cost as reveal_payment;
use crate::ir::ride;
use crate::ir::{
    AbilityChoiceKind, Binding, NumericProperty, RuntimeAbilityReference, ability_play,
};
use crate::ir::{
    Action, CaptureTime, Cardinality, ChoiceDomain, CostSpec, CounterOperation, Decision,
    DecisionKind, Declinable, DiscardMode, Disjointness, ExecutionRef, InputMode,
    InstructionContext, MealRaceOperation, MemberProjection, MoveDestination, MoveReason, Nat,
    Node, ObjectOperand, OwnerZone, PaymentOptionality, PaymentResult, PaymentTime, PlaySlot,
    PlayerRef, PlayerRelation, Position, Posture, Program, ReceiptId, RoleCardinality,
    RuleProcedure, SelectionConstraint, SelectionView, Selector, SliceDirection, TargetRoleSpec,
    Visibility, ZoneKind, ZoneRef,
};
use crate::ir::{AggregateBound, CostStep, KeywordId, PreconditionKind, PreconditionReason};
use crate::ir::{Expr, Int, NumericChange, ObjectReceipt, ObjectReceiptList, ResourceOperation};
use crate::ir::{Resource, x_pp};
use crate::ir::{fusion, shuffle};
use crate::ir::{hand_position, printed_keywords};
use crate::ir::{public_evolve, public_numbers};
use alloc::collections::BTreeSet;
use core::iter::once;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Decision and payment rules share the structural checker state."
)]
impl Checker {
    pub(super) fn program_steps(&mut self, program: &Program) -> Checked {
        if !shuffle::closed(&program.steps) || !hand_position::valid(program) {
            return self.reject();
        }
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
        if printed_keywords::is_stack_increase_choice(decision) {
            self.decision_header(decision)?;
            return self.answer(decision, None);
        }
        let DecisionKind::ResolutionChoice {
            may,
            cardinality,
            precondition,
        } = &decision.kind
        else {
            return Err(self.error(WellFormedReason::DecisionTiming));
        };
        if *may != Declinable::Required
            && !private_look_choice(decision)
            && !revealed_choice(decision)
        {
            return self.reject();
        }
        if let Some(guard) = precondition {
            self.at("precondition", |checker| checker.boolean(&guard.predicate))?;
        }
        self.decision_header(decision)?;
        self.answer(decision, Some(*cardinality))
    }

    pub(super) fn decision_header(&mut self, decision: &Decision) -> Checked {
        if matches!(decision.domain, ChoiceDomain::Abilities { .. })
            && ability_play::choice(decision).is_none()
        {
            return self.reject();
        }
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
        let positioned = hand_position::positioned(decision).is_some();
        if has_roles(&decision.domain)
            && !positioned
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
            checker.domain(
                &decision.domain,
                cardinality,
                matches!(
                    decision.kind,
                    DecisionKind::PlayParameter {
                        slot: PlaySlot::Targets,
                        ..
                    }
                ),
                positioned,
            )
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

    fn ability_domain(
        &mut self,
        holder: &ObjectOperand,
        filter: AbilityChoiceKind,
        binding: Binding<RuntimeAbilityReference>,
        cardinality: Option<Cardinality>,
    ) -> Checked {
        let single = Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        };
        if cardinality != Some(single)
            || !matches!(
                holder,
                ObjectOperand::SourceHolder {} | ObjectOperand::Selected { .. }
            )
        {
            return self.reject();
        }
        self.object(holder)?;
        self.define(binding, BindingKind::Ability(filter))
    }

    fn domain(
        &mut self,
        domain: &ChoiceDomain,
        cardinality: Option<Cardinality>,
        formal_targets: bool,
        positioned: bool,
    ) -> Checked {
        match domain {
            ChoiceDomain::Abilities {
                holder,
                filter,
                binding,
            } => self.ability_domain(holder, *filter, *binding, cardinality),
            ChoiceDomain::Objects {
                constraint,
                binding,
                target_roles,
            } => {
                if cardinality.is_some_and(|cardinality| constraint.cardinality != cardinality) {
                    return Err(self.error(WellFormedReason::DecisionCardinality));
                }
                self.constraint(constraint, formal_targets)?;
                self.define(*binding, BindingKind::Value)?;
                if let Some(roles) = target_roles {
                    self.roles(roles, constraint.cardinality, positioned)?;
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
                    self.at(&index.to_string(), |checker| {
                        checker.domain(child, None, formal_targets, false)
                    })?;
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
        if *decision == ride::parameter(self.env.scope) {
            // Only the fixed DrivePoint cost may inspect the controller's hidden evolve deck.
            let ChoiceDomain::Objects { binding, .. } = &decision.domain else {
                return self.reject();
            };
            self.decision_header(decision)?;
            self.define(*binding, BindingKind::Value)?;
            self.answer_access(&decision.domain, &decision.visibility);
            return Ok(());
        }
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
        self.answer(decision, None)?;
        self.record_field_material(decision);
        Ok(())
    }

    pub(super) fn reveal_cost_decision(&mut self, decision: &Decision) -> Checked {
        if reveal_payment::parameter(decision).is_none() {
            return self.reject();
        }
        self.decision_header(decision)?;
        self.answer(decision, None)?;
        Ok(())
    }

    pub(super) fn retire_reveal_materials(&mut self, costs: &CostSpec) {
        if let CostSpec::Ordered { steps } = costs {
            for step in steps {
                if let Some(Selector::Bound { reference }) =
                    reveal_payment::objects(&step.instruction.action)
                    && self.env.reveal_materials.remove(&reference.key())
                {
                    self.env.bindings.remove(&reference.key());
                    self.env.access.remove(&reference.key());
                }
            }
        }
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

    fn roles(&mut self, spec: &TargetRoleSpec, selected: Cardinality, positioned: bool) -> Checked {
        if positioned {
            for role in &spec.roles {
                self.define(role.binding, BindingKind::Value)?;
            }
            return Ok(());
        }
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

    pub(super) fn constraint(
        &mut self,
        constraint: &SelectionConstraint,
        formal_targets: bool,
    ) -> Checked {
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
        self.aggregate_bounds(&constraint.aggregate_bounds, formal_targets)
    }

    pub(super) fn aggregate_bounds(
        &mut self,
        bounds: &[AggregateBound],
        formal_targets: bool,
    ) -> Checked {
        for (index, bound) in bounds.iter().enumerate() {
            self.at(&format!("aggregate_bounds/{index}"), |checker| {
                let history_limit = bound.property == NumericProperty::Count
                    && formal_targets
                    && !public_numbers::check(&bound.value)
                    && public_numbers::target_count(&bound.value, bound.comparison);
                if bound.property == NumericProperty::Count
                    && !public_numbers::check(&bound.value)
                    && !history_limit
                {
                    return checker.reject();
                }
                // This closed history bound does not inspect the candidate or chooser.
                let chooser = if history_limit {
                    checker.env.chooser.take()
                } else {
                    None
                };
                let checked = checker.integer(&bound.value);
                if history_limit {
                    checker.env.chooser = chooser;
                }
                checked
            })?;
        }
        Ok(())
    }

    pub(super) fn fusion(&mut self, action: &Action) -> Checked {
        let Some(materials) = fusion::materials(action) else {
            return self.reject();
        };
        self.env.chooser = Some(PlayerRef::Controller {});
        self.env.private_choice = true;
        let result = self.constraint(materials, false);
        self.env.chooser = None;
        self.env.private_choice = false;
        result
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
                self.constraint(constraint, false)?;
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
            DiscardMode::ChosenCount { population, count } => {
                if !self.env.choices {
                    return self.reject();
                }
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
            Selector::Extremum { input, .. } => self.selection_visibility(input),
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
                || !super::super::observation_selection::held_by(players, actor) =>
            {
                self.reject()
            }
            Selector::Zone {
                zone: ZoneKind::Evolve,
                ..
            } if public_evolve::selector(selector) => Ok(()),
            Selector::Zone {
                zone: ZoneKind::Deck | ZoneKind::Evolve,
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
                ObjectReceipt::RevealUntil { receipt, .. } => self.visible_receipt(receipt),
                // Every player saw the publicly revealed members.
                ObjectReceipt::Reveal { .. } => Ok(()),
                ObjectReceipt::Token { receipt } => self.visible_receipt(receipt),
                ObjectReceipt::Movement { receipt } if *projection == MemberProjection::Before => {
                    self.movement_before_visible(receipt)
                }
                ObjectReceipt::Movement { receipt } => self.movement_visible(receipt),
                ObjectReceipt::Batch { .. }
                | ObjectReceipt::Fusion { .. }
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
                    // The banish zone is a public, face-up zone.
                    | ZoneKind::Banish
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
            | Selector::Extremum { .. }
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
            && (self.env.play_payments != Some(self.env.scope)
                || (self.env.scope == self.root && !on_paid.steps.is_empty()))
        {
            return Err(self.error(WellFormedReason::DecisionTiming));
        }
        if !self.env.choices && optional == PaymentOptionality::Optional {
            return self.reject();
        }
        let mut paid = None;
        self.isolated(|checker| {
            checker.env.scope = on_paid.scope;
            checker.env.play_payments = None;
            checker.at("costs", |checker| checker.costs(costs))?;
            checker.retire_reveal_materials(costs);
            checker.produce(receipt, TypeId::of::<PaymentResult>())?;
            checker.at("on_paid", |checker| checker.program(on_paid))?;
            if at == PaymentTime::PlayParameter && optional == PaymentOptionality::Required {
                paid = Some(checker.env.clone());
            }
            Ok(())
        })?;
        self.retire_reveal_materials(costs);
        if let Some(paid) = paid {
            self.env.receipts.extend(paid.receipts);
            self.env
                .single_material_moves
                .extend(paid.single_material_moves);
            self.env.single_discards.extend(paid.single_discards);
            self.env.receipt_access.extend(paid.receipt_access);
            self.env
                .movement_before_access
                .extend(paid.movement_before_access);
        }
        self.env
            .receipts
            .insert(receipt, TypeId::of::<PaymentResult>());
        Ok(())
    }

    pub(super) fn play_costs(&mut self, costs: &CostSpec) -> Checked {
        let CostSpec::Ordered { steps } = costs else {
            return self.costs(costs);
        };
        let Some((first, remaining)) = steps.split_first() else {
            return self.costs(costs);
        };
        let Some(guard) = &first.instruction.precondition else {
            return self.costs(costs);
        };
        if guard.kind != PreconditionKind::CardTextPrerequisite
            || guard.protects != first.instruction.id
            || guard.reason != (PreconditionReason::CardText {})
        {
            return Err(self.error(WellFormedReason::PreconditionTiming));
        }
        self.at("0/precondition", |checker| {
            checker.boolean(&guard.predicate)
        })?;
        // A use prerequisite gates the whole play; generic payment steps stay unconditional.
        let mut unconditional = first.clone();
        unconditional.instruction.precondition = None;
        self.costs(&CostSpec::Ordered {
            steps: once(unconditional)
                .chain(remaining.iter().cloned())
                .collect(),
        })
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
                        if !keyword_cost(step) {
                            return checker.reject();
                        }
                        if step.instruction.context != InstructionContext::Cost {
                            return Err(checker.error(WellFormedReason::ExpectedCost));
                        }
                        if step.instruction.precondition.is_some() {
                            return Err(checker.error(WellFormedReason::PreconditionTiming));
                        }
                        let valid = matches!(&step.instruction.action,
                            Action::Discard { .. }
                            | Action::ScheduleTurn { player: PlayerRef::Controller {}, operation: TurnOperation::SkipNext {}, .. }
                            | Action::Fusion { .. }
                            | Action::MealRace { operation: MealRaceOperation::Meal, .. }
                            | Action::Counter { operation: CounterOperation::Remove {}, .. }
                            | Action::Posture { posture: Posture::Acted, .. })
                            || matches!(&step.instruction.action, Action::Reveal { objects: Selector::Bound { reference } }
                                if checker.env.reveal_materials.contains(&reference.key()))
                            || step.instruction.action == ride::attachment(checker.env.scope)
                            || matches!(&step.instruction.action, Action::Resource { operation: ResourceOperation::Pay, amount: Expr::Literal(Int(value)), .. } if *value >= 0)
                            || matches!(&step.instruction.action, Action::Resource { player: PlayerRef::Controller {}, resource: Resource::PpCurrent, operation: ResourceOperation::Pay, amount } if x_pp::amount(amount).is_some())
                            || matches!(&step.instruction.action, Action::Life { change: NumericChange::Add { amount: Expr::Literal(Int(value)) }, .. } if *value <= 0);
                        let source_move = matches!(&step.instruction.action,
                            Action::Move { objects: Selector::Objects { objects }, movement, reason: MoveReason::Put }
                            if objects == &[ObjectOperand::SourceHolder {}]
                                && movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery })
                                && movement.position.is_none() && movement.order.is_none()
                                && movement.initial_posture.is_none() && movement.entry_effects.is_empty())
                            || matches!(&step.instruction.action,
                                Action::Banish { objects: Selector::Objects { objects } }
                                    if objects == &[ObjectOperand::SourceHolder {}])
                            || hand_source_to_ex(&step.instruction.action);
                        let selected_move = matches!(&step.instruction.action,
                            Action::Move { objects: Selector::Bound { .. }, movement, reason: MoveReason::Put }
                                if (matches!(movement.to, MoveDestination::Owners { zone: OwnerZone::Cemetery | OwnerZone::Hand })
                                    || matches!(movement.to, MoveDestination::Shared { zone: ZoneRef { player: PlayerRef::Controller {}, kind: ZoneKind::Ex } }))
                                    && movement.position.is_none() && movement.order.is_none()
                                    && movement.initial_posture.is_none() && movement.entry_effects.is_empty())
                            || matches!(&step.instruction.action, Action::Banish { objects: Selector::Bound { .. } });
                        let private_bottom = matches!(reveal_payment::bottom(&step.instruction.action),
                            Some(Selector::Bound { reference }) if checker.env.reveal_materials.contains(&reference.key()));
                        let deck_top_banish = matches!(&step.instruction.action,
                            Action::Banish { objects: Selector::Ordered {
                                zone: ZoneRef { player: PlayerRef::Controller {}, kind: ZoneKind::Deck },
                                start: Position::Top {}, count: Expr::Literal(Nat(count)),
                                direction: SliceDirection::Forward,
                            }} if *count > 0);
                        if !valid && !source_move && !selected_move && !private_bottom && !deck_top_banish {
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

/// 「手札のこれをEXエリアに置く」: the activating hand card moves to its controller's EX.
fn hand_source_to_ex(action: &Action) -> bool {
    let Action::Move {
        objects: Selector::Intersection { sets },
        movement,
        reason: MoveReason::Put,
    } = action
    else {
        return false;
    };
    matches!(sets.as_slice(), [
        Selector::Zone { players, zone: ZoneKind::Hand, view: SelectionView::Current {}, .. },
        Selector::Objects { objects },
    ] if players.relation == PlayerRelation::SelfPlayer
        && players.relative_to == (PlayerRef::Controller {})
        && objects == &[ObjectOperand::SourceHolder {}])
        && movement.to
            == (MoveDestination::Owners {
                zone: OwnerZone::Ex,
            })
        && movement.position.is_none()
        && movement.order.is_none()
        && movement.initial_posture.is_none()
        && movement.entry_effects.is_empty()
}

fn has_roles(domain: &ChoiceDomain) -> bool {
    match domain {
        ChoiceDomain::Objects { target_roles, .. } => target_roles.is_some(),
        ChoiceDomain::Compound { domains } => domains.iter().any(has_roles),
        ChoiceDomain::Abilities { .. }
        | ChoiceDomain::Recipients { .. }
        | ChoiceDomain::EndPhaseGuardAct { .. }
        | ChoiceDomain::Modes { .. }
        | ChoiceDomain::Order { .. }
        | ChoiceDomain::Distribution { .. }
        | ChoiceDomain::Integer { .. }
        | ChoiceDomain::Names { .. }
        | ChoiceDomain::QuickActionOrPass { .. } => false,
    }
}

fn keyword_cost(step: &CostStep) -> bool {
    match step.keyword {
        None => true,
        Some(KeywordId::EarthRite) => matches!(
            &step.instruction.action,
            Action::Counter { kind, operation: CounterOperation::Remove {}, amount: Expr::Literal(amount), .. }
                if kind.0 == "stack_counter" && amount.0 > 0
        ),
        Some(_) => false,
    }
}
