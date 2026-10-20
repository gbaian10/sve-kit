use super::{BindingKind, Checked, Checker, TypeId, WellFormedReason};
use crate::ir::{
    Action, Cardinality, ChoiceDomain, CostSpec, CounterOperation, Decision, DecisionKind,
    Declinable, DiscardMode, Disjointness, InstructionContext, Node, PaymentOptionality,
    PaymentResult, PaymentTime, PlayerRef, PlayerRelation, Posture, Program, ReceiptId,
    SelectionConstraint, SelectionView, Selector, Visibility, ZoneKind,
};

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
            let public_scalars = self.env.public_scalars.clone();
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
                        checker.node(node)
                    })
                })?;
                // A skipped instruction never supplies its guarded selection to later nodes.
                self.env.bindings = bindings;
                self.env.public_scalars = public_scalars;
            }
        }
        Ok(())
    }

    pub(super) fn decision(&mut self, decision: &Decision) -> Checked {
        if !self.env.choices {
            return self.reject();
        }
        if !self.decisions.insert(decision.id) {
            return Err(self.error(WellFormedReason::DuplicateDecision));
        }
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
        self.cardinality(*cardinality)?;
        self.player(&decision.actor)?;
        self.visibility(&decision.visibility)?;
        if let Some(guard) = precondition {
            self.at("precondition", |checker| checker.boolean(&guard.predicate))?;
        }
        self.env.chooser = Some(decision.actor.clone());
        self.env.private_choice = matches!(
            &decision.visibility,
            Visibility::Only { players } if players.relation == PlayerRelation::SelfPlayer
                && players.relative_to == decision.actor
        );
        let result = self.at("domain", |checker| {
            checker.domain(&decision.domain, *cardinality)
        });
        self.env.chooser = None;
        self.env.private_choice = false;
        result?;
        if decision.visibility == (Visibility::Public {})
            && let ChoiceDomain::Integer { binding, .. } = &decision.domain
        {
            self.env.public_scalars.insert(binding.key());
        }
        Ok(())
    }

    fn visibility(&mut self, visibility: &Visibility) -> Checked {
        match visibility {
            Visibility::Public {} | Visibility::AuthorityOnly {} => Ok(()),
            Visibility::Only { players } => self.players(players),
        }
    }

    fn domain(&mut self, domain: &ChoiceDomain, cardinality: Cardinality) -> Checked {
        match domain {
            ChoiceDomain::Objects {
                constraint,
                binding,
                target_roles: None,
            } => {
                if constraint.cardinality != cardinality {
                    return Err(self.error(WellFormedReason::DecisionCardinality));
                }
                self.constraint(constraint)?;
                self.define(*binding, BindingKind::Value)
            }
            ChoiceDomain::Integer {
                cardinality: bounds,
                binding,
            } => {
                self.cardinality(*bounds)?;
                if *bounds != cardinality {
                    return Err(self.error(WellFormedReason::DecisionCardinality));
                }
                self.define(*binding, BindingKind::Value)
            }
            ChoiceDomain::Objects { .. }
            | ChoiceDomain::Modes { .. }
            | ChoiceDomain::Order { .. }
            | ChoiceDomain::Distribution { .. }
            | ChoiceDomain::Names { .. }
            | ChoiceDomain::Compound { .. }
            | ChoiceDomain::QuickActionOrPass { .. } => self.reject(),
        }
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
        for (index, bound) in constraint.aggregate_bounds.iter().enumerate() {
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
            | Selector::Ordered { .. }
            | Selector::Bound { .. }
            | Selector::Captured { .. }
            | Selector::ReceiptMembers { .. }
            | Selector::CollectedMembers { .. }
            | Selector::Created { .. } => self.reject(),
            Selector::MoveAfterAll { .. }
            | Selector::MoveAfter { .. }
            | Selector::Zone {
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
        if at != PaymentTime::ResolutionChoice {
            return Err(self.error(WellFormedReason::DecisionTiming));
        }
        if !self.env.choices && optional == PaymentOptionality::Optional {
            return self.reject();
        }
        let CostSpec::Ordered { steps } = costs else {
            return self.reject();
        };
        if steps.is_empty() {
            return self.reject();
        }
        self.isolated(|checker| {
            checker.env.scope = on_paid.scope;
            checker.at("costs", |checker| {
                for (index, step) in steps.iter().enumerate() {
                    checker.at(&index.to_string(), |checker| {
                        if step.instruction.context != InstructionContext::Cost {
                            return Err(checker.error(WellFormedReason::ExpectedCost));
                        }
                        if !matches!(
                            &step.instruction.action,
                            Action::Discard { .. }
                                | Action::Counter {
                                    operation: CounterOperation::Remove {},
                                    ..
                                }
                                | Action::Posture {
                                    posture: Posture::Acted,
                                    ..
                                }
                        ) {
                            return checker.reject();
                        }
                        checker.instruction_in(&step.instruction, InstructionContext::Cost)?;
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
                Ok(())
            })?;
            checker.produce(receipt, TypeId::of::<PaymentResult>())?;
            checker.at("on_paid", |checker| checker.program(on_paid))
        })?;
        self.env
            .receipts
            .insert(receipt, TypeId::of::<PaymentResult>());
        Ok(())
    }
}
