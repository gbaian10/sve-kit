//! Every pending hand cost shares the same physical pool before payment.

use super::{
    Execution, Game, Result, invalid, seat_name, selection, source_discard, source_to_ex,
    unsupported,
};
use crate::game::internal::card::selection::groups::GroupConstraints;
use crate::game::internal::conditions::{ConditionPlan, Policy};
use crate::game::internal::discard::HandWait;
use crate::game::internal::player::CandidateLinks;
use crate::game::internal::{DistributionMember, InputDomain, TargetDomain, Wait, search};
use crate::ir::{Action, CostSpec, Instruction, Nat, ObjectOperand, ObjectRef, Selector};

struct Choice {
    wait: HandWait,
    domain: TargetDomain,
}

fn unavailable() -> crate::EngineFailure {
    crate::EngineFailure::IllegalPlay("no disjoint hand cost selection".into())
}

fn witness(choices: &[Choice]) -> Result<Vec<Vec<ObjectRef>>> {
    if let [choice] = choices {
        return Ok(vec![choice.wait.witness_members()?]);
    }
    let proofs = choices
        .iter()
        .map(|choice| choice.wait.proof().clone())
        .collect::<Vec<_>>();
    let assignment = GroupConstraints::joint_witness(&proofs)?.ok_or_else(unavailable)?;
    if !assignment.iter().zip(choices).all(|(members, choice)| {
        super::contains(
            choice.domain.cardinality,
            Nat(u64::try_from(members.len()).unwrap_or(u64::MAX)),
        ) && choice.wait.proof().accepts(members)
    }) {
        return Err(invalid("joint hand cost proof"));
    }
    Ok(assignment)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Hand choices share the card play transaction."
)]
impl Game {
    pub(super) fn internal_hand_payment_question(
        &mut self,
        execution: &mut Execution,
        costs: &[&CostSpec],
    ) -> Result<bool> {
        let choices = self.internal_hand_choices(execution, costs)?;
        let _assignment = witness(&choices)?;
        let Some(choice) = choices.into_iter().next() else {
            return Ok(false);
        };
        self.internal_wait(
            execution,
            choice.wait.decision(),
            execution.context.controller,
            InputDomain::CostMaterials(choice.domain),
            Wait::CostDiscard(choice.wait),
        )?;
        Ok(true)
    }

    pub(super) fn internal_probe_hand_payments(
        &self,
        execution: &mut Execution,
        costs: &[&CostSpec],
    ) -> Result<()> {
        let choices = self.internal_hand_choices(execution, costs)?;
        let assignment = witness(&choices)?;
        for (choice, members) in choices.into_iter().zip(assignment) {
            let handles = choice.wait.handles(&members)?;
            let validated = choice.wait.validate(self, execution, &handles)?;
            choice.wait.record(execution, validated);
        }
        Ok(())
    }

    fn internal_hand_choices(
        &self,
        execution: &Execution,
        costs: &[&CostSpec],
    ) -> Result<Vec<Choice>> {
        let mut instructions = Vec::new();
        for cost in costs {
            let CostSpec::Ordered { steps } = cost else {
                return Err(unsupported("ordered hand costs"));
            };
            instructions.extend(steps.iter().map(|step| &step.instruction));
        }
        let mut reserved = false;
        for instruction in &instructions {
            let action = &instruction.action;
            reserved |= source_discard(action)
                || source_to_ex(action)
                || matches!(action, Action::Fusion { .. })
                || matches!(action, Action::Banish { objects: Selector::Objects { objects } }
                    if objects == &[ObjectOperand::SourceHolder {}]);
            if matches!(action, Action::Discard { .. }) && !source_discard(action) {
                reserved |= selection(action)?.source;
            }
        }
        instructions
            .into_iter()
            .filter(|instruction| {
                matches!(instruction.action, Action::Discard { .. })
                    && !source_discard(&instruction.action)
                    && !execution.cost_materials.contains_key(&instruction.receipt)
            })
            .map(|instruction| self.internal_hand_choice(execution, instruction, reserved))
            .collect()
    }

    fn internal_hand_choice(
        &self,
        execution: &Execution,
        instruction: &Instruction,
        reserved: bool,
    ) -> Result<Choice> {
        let plan = selection(&instruction.action)?;
        let seat = seat_name(execution.context.controller)?;
        if plan.source {
            let source = self.object(&execution.context.source)?;
            if source.zone != "hand" || source.owner != seat || source.controller != seat {
                return Err(unavailable());
            }
        }
        let predicates = plan
            .groups
            .iter()
            .map(|group| {
                let Selector::Zone {
                    binding, predicate, ..
                } = &group.selector
                else {
                    return Err(unsupported("hand cost selector"));
                };
                ConditionPlan::compile(predicate, *binding, Policy::Search)?.prepare(None)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut groups = GroupConstraints {
            members: vec![Vec::new(); predicates.len()],
            bounds: plan.groups.iter().map(|group| group.cardinality).collect(),
        };
        let mut members = Vec::new();
        let mut candidates = Vec::new();
        for id in self.zone_ids(seat, "hand") {
            if reserved && id == execution.context.source {
                continue;
            }
            let object = self.object(&id)?;
            if object.owner != seat || object.controller != seat {
                return Err(unsupported("foreign hand owner"));
            }
            let member = execution.reference(self, &id)?;
            if execution
                .cost_materials
                .values()
                .any(|chosen| chosen.contains(&member))
            {
                continue;
            }
            let mut eligible = false;
            for (predicate, group) in predicates.iter().zip(&mut groups.members) {
                if search::matches_candidate(self, &id, predicate)? {
                    group.push(member);
                    eligible = true;
                }
            }
            if eligible {
                candidates.push(DistributionMember {
                    handle: u64::try_from(members.len()).map_err(invalid)?,
                    player: execution.context.controller,
                    card: Some(object.card.clone()),
                    name: Some(self.card_name(&id)?),
                });
                members.push(member);
            }
        }
        let links = CandidateLinks::capture(self, execution, members.iter().copied())?;
        let wait = if plan.groups.len() == 1 {
            HandWait::strict(
                links,
                Box::new(instruction.clone()),
                members,
                plan.cardinality,
            )
        } else {
            HandWait::grouped(
                links,
                Box::new(instruction.clone()),
                members,
                plan.cardinality,
                groups,
            )
        };
        Ok(Choice {
            wait,
            domain: TargetDomain {
                members: candidates,
                cardinality: plan.cardinality,
                distinct_names: false,
                original_costs: None,
                roles: vec![],
            },
        })
    }
}
