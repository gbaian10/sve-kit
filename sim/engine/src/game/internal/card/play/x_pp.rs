use super::{
    Execution, Game, InputDomain, PlayState, Result, Slot, Wait, int, invalid, seat_name,
    unsupported,
};
use crate::game::player::IntegerSource;
use crate::ir::x_pp;
use crate::ir::{
    Action, Binding, Cardinality, CostSpec, Expr, Int, Nat, PlayerRef, Resource, ResourceOperation,
};

#[derive(Default)]
pub(super) struct Budget {
    pub(super) fixed: i64,
    pub(super) copies: u64,
}
impl Budget {
    pub(super) fn include(
        &mut self,
        costs: &CostSpec,
        binding: Option<Binding<Nat>>,
        read_pp: impl Fn(Int) -> Result<Int>,
    ) -> Result<()> {
        self.include_resolved(costs, binding, read_pp, |_| None)
    }

    pub(super) fn include_resolved(
        &mut self,
        costs: &CostSpec,
        binding: Option<Binding<Nat>>,
        read_pp: impl Fn(Int) -> Result<Int>,
        resolved: impl Fn(Binding<Nat>) -> Option<Nat>,
    ) -> Result<()> {
        let CostSpec::Ordered { steps } = costs else {
            return Err(unsupported("X cost alternatives"));
        };
        for step in steps {
            let Action::Resource {
                player: PlayerRef::Controller {},
                resource: Resource::PpCurrent,
                operation: ResourceOperation::Pay,
                amount,
            } = &step.instruction.action
            else {
                continue;
            };
            if let Expr::Literal(value) = amount {
                let Int(value) = read_pp(*value)?;
                self.fixed = self
                    .fixed
                    .checked_add(value)
                    .ok_or_else(|| invalid("X fixed PP overflow"))?;
            } else if binding.is_some() && x_pp::amount(amount) == binding {
                self.copies = self
                    .copies
                    .checked_add(1)
                    .ok_or_else(|| invalid("X PP copies overflow"))?;
            } else if let Some(chosen) = x_pp::amount(amount).and_then(&resolved) {
                let Int(value) = read_pp(Int(i64::try_from(chosen.0).map_err(invalid)?))?;
                self.fixed = self
                    .fixed
                    .checked_add(value)
                    .ok_or_else(|| invalid("X fixed PP overflow"))?;
            } else {
                return Err(unsupported("X PP amount"));
            }
        }
        Ok(())
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "X uses the same play plan and quoted PP as ordinary payment."
)]
impl Game {
    pub(super) fn internal_x_parameter(
        &mut self,
        execution: &mut Execution,
        play: &mut PlayState,
    ) -> Result<bool> {
        let Some(declaration) = play.parameters.front() else {
            return Ok(false);
        };
        let Some(binding) = x_pp::parameter(declaration) else {
            return Ok(false);
        };
        if !play.groups.is_empty() || play.exchange.is_some() || play.resource_entry.is_some() {
            return Err(unsupported("X beside modes or card cost exchange"));
        }
        let mut budget = Budget::default();
        let resolved = |parameter: Binding<Nat>| match execution.slots.get(&parameter.key()) {
            Some(Slot::Natural(value)) => Some(*value),
            None if play
                .parameters
                .iter()
                .any(|decision| x_pp::parameter(decision) == Some(parameter)) =>
            {
                Some(Nat(0))
            }
            _ => None,
        };
        budget.include_resolved(
            &play.costs,
            Some(binding),
            |printed| self.internal_card_pp_amount(execution, execution.spell, printed),
            resolved,
        )?;
        budget.include_resolved(&play.mode_costs, Some(binding), Ok, resolved)?;
        let mut costs = vec![&play.costs, &play.mode_costs];
        if let Some((_, additional)) = play.additional.as_deref() {
            budget.include_resolved(&additional.play.costs, Some(binding), Ok, resolved)?;
            costs.push(&additional.play.costs);
        }
        for payment in &play.optional {
            if payment.accepted == Some(true) {
                budget.include_resolved(&payment.costs, Some(binding), Ok, resolved)?;
                costs.push(&payment.costs);
            }
        }
        let counter_maximum = super::counter_budget::maximum(self, execution, binding, &costs)?;
        let decision = play
            .parameters
            .pop_front()
            .ok_or_else(|| invalid("missing X parameter"))?;
        if budget.copies == 0 && counter_maximum.is_none() {
            execution.slots.insert(binding.key(), Slot::Natural(Nat(0)));
            return Ok(false);
        }
        let current = int(&self.player(seat_name(execution.context.controller)?)?.pp["current"]);
        let available = current
            .checked_sub(budget.fixed)
            .ok_or_else(|| invalid("X PP budget overflow"))?;
        if available < 0 {
            return Err(crate::EngineFailure::IllegalPlay(
                "insufficient fixed PP before X".into(),
            ));
        }
        let available = u64::try_from(available).map_err(invalid)?;
        let pp_maximum = if budget.copies == 0 {
            None
        } else {
            Some(
                available
                    .checked_div(budget.copies)
                    .ok_or_else(|| invalid("X PP copies"))?,
            )
        };
        let maximum = match (pp_maximum, counter_maximum) {
            (Some(pp), Some(counters)) => pp.min(counters),
            (Some(value), None) | (None, Some(value)) => value,
            (None, None) => return Err(invalid("missing X payment")),
        };
        let bounds = Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(maximum)),
        };
        self.internal_wait(
            execution,
            decision.id,
            execution.context.controller,
            InputDomain::Integer(bounds),
            Wait::Integer {
                binding: super::super::super::declaration::NumberBinding::Natural(binding),
                bounds,
                source: IntegerSource::XCost,
                purpose: super::super::super::player::purpose(&decision.kind),
                declaration: None,
            },
        )?;
        Ok(true)
    }
}
