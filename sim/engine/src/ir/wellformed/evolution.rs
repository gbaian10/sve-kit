use super::{BindingKind, Checked, Checker, WellFormedError, WellFormedReason};
use crate::ir::{
    Ability, AbilityKind, AbilityOrigin, Action, Binding, CardName, Cardinality, ChoiceDomain,
    Control, CostSpec, Decision, DecisionKind, Disjointness, EvolutionMode, ExecutionRef, Expr,
    InputMode, InstructionContext, Int, KeywordId, NamesRead, Nat, Node, ObjectOperand, ObjectView,
    PlaySlot, PlayerRef, PlayerRelation, PlayerSet, PreconditionKind, PreconditionReason,
    Predicate, Resource, ResourceOperation, SelectionSnapshot, SelectionView, Selector,
    TextQuantifier, Visibility, ZoneKind,
};
use crate::ir::{ObjectRef, Program, effect_evolution};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The temporary single-face evolution entry shares the closed checker state."
)]
impl Checker {
    pub(super) fn evolution_entry(&mut self, ability: &Ability) -> Checked {
        let Some(play) = &ability.play else {
            return self.reject();
        };
        if ability.kind != AbilityKind::Activated
            || ability.active_zones != [ZoneKind::Field]
            || ability.source.holder != (ObjectOperand::SourceHolder {})
            || ability.source.controller != (PlayerRef::Controller {})
            || !matches!(&ability.source.origin,
                AbilityOrigin::Keyword { keyword: KeywordId::Evolution, origin }
                    if matches!(origin.as_ref(), AbilityOrigin::Printed { .. }))
            || !ability.event_captures.is_empty()
            || !play.captures.is_empty()
        {
            return self.reject();
        }
        let [face] = play.parameters.as_slice() else {
            return self.reject();
        };
        self.play_entry = Some(self.env.clone());
        let selection = self.at("play/parameters/0", |checker| {
            checker.evolution_face(face, false)
        })?;
        self.at("play/costs", |checker| {
            checker.evolution_costs(&play.costs, selection)
        })?;
        let Some(body) = &ability.body else {
            return self.reject();
        };
        let [projection] = body.steps.as_slice() else {
            return self.reject();
        };
        let Node::Control {
            control:
                Control::EachObject {
                    input,
                    binding,
                    body: projected,
                    order: None,
                },
        } = &projection.node
        else {
            return self.reject();
        };
        if *input
            != (Selector::Bound {
                reference: selection,
            })
        {
            return self.reject();
        }
        let [step] = projected.steps.as_slice() else {
            return self.reject();
        };
        let Node::Instruction { instruction } = &step.node else {
            return self.reject();
        };
        if instruction.actor != ability.source.controller
            || instruction.precondition.is_some()
            || !matches!(&instruction.action,
                Action::Evolve {
                    subject: ObjectOperand::SourceHolder {},
                    form,
                    mode: EvolutionMode::Normal,
                    ..
                } if form.object == (ObjectOperand::Bound { binding: *binding }))
        {
            return self.reject();
        }
        // Exact-one iteration is a temporary projection, not a printed per-object effect.
        self.env.evolution_form = Some(*binding);
        self.resolution_entry = self.env.clone();
        self.at("body/steps", |checker| checker.program_steps(body))
    }

    fn evolution_face(
        &mut self,
        decision: &Decision,
        resolution: bool,
    ) -> Result<Binding<Selector>, WellFormedError> {
        if (!resolution
            && decision.kind
                != (DecisionKind::PlayParameter {
                    slot: PlaySlot::Face,
                    selection_snapshot: SelectionSnapshot::AtDecision,
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                }))
            || (resolution && !effect_evolution::is_choice(decision))
            || decision.actor != (PlayerRef::Controller {})
            || decision.visibility
                != (Visibility::Only {
                    players: controller(),
                })
            || !decision.captures.is_empty()
        {
            return Err(self.error(WellFormedReason::UnsupportedShape));
        }
        let ChoiceDomain::Objects {
            constraint,
            binding,
            target_roles: None,
        } = &decision.domain
        else {
            return Err(self.error(WellFormedReason::UnsupportedShape));
        };
        let [group] = constraint.groups.as_slice() else {
            return Err(self.error(WellFormedReason::UnsupportedShape));
        };
        if constraint.cardinality != one()
            || group.cardinality != one()
            || constraint.distinct_by.is_some()
            || constraint.disjoint != Disjointness::Disjoint
            || !constraint.aggregate_bounds.is_empty()
        {
            return Err(self.error(WellFormedReason::UnsupportedShape));
        }
        self.evolution_candidates(&group.selector)?;
        self.decision_header(decision)?;
        self.define(*binding, BindingKind::Value)?;
        self.answer_access(&decision.domain, &decision.visibility);
        Ok(*binding)
    }

    pub(super) fn effect_evolution(
        &mut self,
        body: &Program,
        decision: &Decision,
        form: Binding<ObjectRef>,
    ) -> Checked {
        if !self.scopes.insert(body.scope) {
            return Err(self.error(WellFormedReason::DuplicateScope));
        }
        self.env.scope = body.scope;
        self.evolution_face(decision, true)?;
        self.env.evolution_form = Some(form);
        let iteration = body
            .steps
            .get(1)
            .ok_or_else(|| self.error(WellFormedReason::InvalidEntry))?;
        self.located(iteration, Self::node)
    }

    fn evolution_candidates(&mut self, selector: &Selector) -> Checked {
        let Selector::Zone {
            players,
            zone: ZoneKind::Evolve,
            view: SelectionView::Current {},
            binding,
            predicate,
        } = selector
        else {
            return self.reject();
        };
        let same_name = Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: names(ObjectOperand::Bound { binding: *binding }),
            right: names(ObjectOperand::SourceHolder {}),
        }));
        if *players != controller() || **predicate != same_name {
            return self.reject();
        }
        // The complete predicate shape is checked above; its candidate never escapes the domain.
        self.isolated(|checker| {
            let scope = binding.key().0;
            if scope != checker.env.scope && !checker.scopes.insert(scope) {
                return Err(checker.error(WellFormedReason::DuplicateScope));
            }
            checker.env.scope = scope;
            checker.define(*binding, BindingKind::Value)
        })
    }

    fn evolution_costs(&mut self, costs: &CostSpec, selection: Binding<Selector>) -> Checked {
        let CostSpec::Ordered { steps } = costs else {
            return self.reject();
        };
        let [reveal, pp] = steps.as_slice() else {
            return self.reject();
        };
        if reveal.instruction.action
            != (Action::Reveal {
                objects: Selector::Bound {
                    reference: selection,
                },
            })
            || !matches!(&pp.instruction.action,
            Action::Resource {
                player: PlayerRef::Controller {},
                resource: Resource::PpCurrent,
                operation: ResourceOperation::Pay,
                amount: Expr::Literal(Int(value)),
            } if *value >= 0)
        {
            return self.reject();
        }
        for (index, cost) in steps.iter().enumerate() {
            if cost.materials.is_some() || cost.instruction.actor != (PlayerRef::Controller {}) {
                return self.reject();
            }
            self.at(&index.to_string(), |checker| {
                if let Some(guard) = &cost.instruction.precondition {
                    if index != 0
                        || guard.kind != PreconditionKind::CardTextPrerequisite
                        || guard.protects != cost.instruction.id
                        || guard.reason != (PreconditionReason::CardText {})
                    {
                        return checker.reject();
                    }
                    checker.at("precondition", |checker| checker.boolean(&guard.predicate))?;
                }
                // An intrinsic qualification gates the entire play before its unconditional reveal.
                let mut instruction = cost.instruction.clone();
                instruction.precondition = None;
                checker.instruction_in(&instruction, InstructionContext::Cost, false)
            })?;
        }
        Ok(())
    }
}

const fn one() -> Cardinality {
    Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    }
}

const fn controller() -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: PlayerRef::Controller {},
        cardinality: one(),
        text_quantifier: TextQuantifier::Implicit,
    }
}

fn names(object: ObjectOperand) -> Expr<Vec<CardName>> {
    Expr::Live(Box::new(NamesRead::Names {
        object: ObjectView::Current {
            object: Box::new(object),
        },
    }))
}
