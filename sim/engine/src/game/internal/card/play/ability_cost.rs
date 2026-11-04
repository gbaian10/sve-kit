use super::{Execution, PlayState, Result, Slot, unsupported};
use crate::ir::{AbilityCostPolicy, ChoiceDomain, CostSpec, Decision, DecisionKind, Nat, PlaySlot};
use core::{mem::take, slice};

pub(in crate::game::internal) fn seed_original_x(
    execution: &mut Execution,
    parameters: &[Decision],
) -> Result<()> {
    for decision in parameters {
        if matches!(
            decision.kind,
            DecisionKind::PlayParameter {
                slot: PlaySlot::X,
                ..
            }
        ) {
            let ChoiceDomain::Integer { binding, .. } = decision.domain else {
                return Err(unsupported("original ability X parameter"));
            };
            if !decision.captures.is_empty() {
                return Err(unsupported("original X captures"));
            }
            execution.slots.insert(binding.key(), Slot::Natural(Nat(0)));
        }
    }
    Ok(())
}

const fn empty_costs() -> CostSpec {
    CostSpec::Ordered { steps: vec![] }
}

fn remaining_parameters(
    parameters: impl IntoIterator<Item = Decision>,
    execution: &mut Execution,
) -> Result<Vec<Decision>> {
    let mut retained = vec![];
    for decision in parameters {
        match decision.kind {
            DecisionKind::PlayParameter {
                slot: PlaySlot::CostMaterials,
                ..
            } => {
                let ChoiceDomain::Objects { binding, .. } = decision.domain else {
                    return Err(unsupported("original ability materials"));
                };
                execution.slots.insert(
                    binding.key(),
                    Slot::Objects {
                        members: vec![],
                        on_field_at_capture: false,
                    },
                );
            }
            DecisionKind::PlayParameter {
                slot: PlaySlot::X, ..
            } => {
                seed_original_x(execution, slice::from_ref(&decision))?;
            }
            _ => retained.push(decision),
        }
    }
    Ok(retained)
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Original-cost authorization changes the common play plan, not its executor."
)]
impl PlayState {
    pub(super) fn waive_original_costs(&mut self, execution: &mut Execution) -> Result<()> {
        if self.cost_policy != AbilityCostPolicy::WaiveOriginal {
            return Ok(());
        }
        if self.resource_entry.is_some() || self.exchange.is_some() {
            return Err(unsupported("waived intrinsic resource or exchange play"));
        }
        self.costs = empty_costs();
        self.mode_costs = empty_costs();
        self.parameters = remaining_parameters(take(&mut self.parameters), execution)?.into();
        for payment in &mut self.optional {
            payment.costs = empty_costs();
            payment.accepted = Some(true);
        }
        for group in self.groups.iter_mut().chain(self.modes.iter_mut()) {
            for mode in &mut group.modes {
                mode.costs = empty_costs();
                mode.parameters = remaining_parameters(take(&mut mode.parameters), execution)?;
            }
        }
        Ok(())
    }
}
