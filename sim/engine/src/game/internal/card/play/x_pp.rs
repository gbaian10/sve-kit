use super::{
    Execution, Game, InputDomain, PlayState, Result, Slot, Wait, int, invalid, seat_name,
    unsupported,
};
use crate::ir::x_pp;
use crate::ir::{
    Action, Binding, Cardinality, CostSpec, Expr, Int, Nat, PlayerRef, Resource, ResourceOperation,
};

#[derive(Default)]
struct Budget {
    fixed: i64,
    copies: u64,
}
impl Budget {
    fn include(
        &mut self,
        game: &Game,
        execution: &Execution,
        costs: &CostSpec,
        binding: Binding<Nat>,
        card_cost: bool,
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
                let Int(value) = game.internal_card_pp_amount(execution, card_cost, *value)?;
                self.fixed = self
                    .fixed
                    .checked_add(value)
                    .ok_or_else(|| invalid("X fixed PP overflow"))?;
            } else if x_pp::amount(amount) == Some(binding) {
                self.copies = self
                    .copies
                    .checked_add(1)
                    .ok_or_else(|| invalid("X PP copies overflow"))?;
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
        let Some(parameter) = play.parameters.front() else {
            return Ok(false);
        };
        let Some(binding) = x_pp::parameter(parameter) else {
            return Ok(false);
        };
        if !play.groups.is_empty() || play.exchange.is_some() || play.resource_entry.is_some() {
            return Err(unsupported("X beside modes or card cost exchange"));
        }
        let mut budget = Budget::default();
        budget.include(self, execution, &play.costs, binding, execution.spell)?;
        budget.include(self, execution, &play.mode_costs, binding, false)?;
        if let Some((_, additional)) = play.additional.as_deref() {
            budget.include(self, execution, &additional.play.costs, binding, false)?;
        }
        for payment in &play.optional {
            if payment.accepted == Some(true) {
                budget.include(self, execution, &payment.costs, binding, false)?;
            }
        }
        let decision = play
            .parameters
            .pop_front()
            .ok_or_else(|| invalid("missing X parameter"))?;
        if budget.copies == 0 {
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
        let maximum = u64::try_from(available)
            .map_err(invalid)?
            .checked_div(budget.copies)
            .ok_or_else(|| invalid("missing X PP payment"))?;
        let bounds = Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(maximum)),
        };
        self.internal_wait(
            execution,
            decision.id,
            execution.context.controller,
            InputDomain::Integer(bounds),
            Wait::Integer { binding, bounds },
        )?;
        Ok(true)
    }
}
