//! Earth Rite retains its keyword identity while delegating counter payment to the common cost.
use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, seat_name, unsupported};
use crate::ir::{
    Action, Bool, CostStep, CounterOperation, EarthRitePaidComponent, Expr, Holder, KeywordId,
    MoveMethod, Nat, PaidComponent, PaymentEvent, PaymentResult, PaymentTime, RuntimeHolder,
    RuntimeReceiptId, ZoneKind,
};
use core::slice::from_ref;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "The keyword adds its rule movement to an ordinary counter cost."
)]
impl Game {
    pub(super) fn check_keyword_cost(&self, execution: &Execution, step: &CostStep) -> Result<()> {
        let Some(keyword) = step.keyword else {
            return Ok(());
        };
        let Action::Counter {
            holder: Holder::Object { object },
            kind,
            operation: CounterOperation::Remove {},
            amount: Expr::Literal(Nat(amount)),
        } = &step.instruction.action
        else {
            return Err(unsupported("keyword counter cost"));
        };
        if keyword != KeywordId::EarthRite || kind.0 != "stack_counter" || *amount == 0 {
            return Err(unsupported("Earth Rite counter cost"));
        }
        let reference = execution.object(self, object)?;
        let id = execution.object_id(reference)?;
        let holder = self.object(id)?;
        if holder.generation != reference.generation.0
            || holder.zone != "field"
            || holder.controller != seat_name(execution.context.controller)?
            || !matches!(
                self.object_type(id)?,
                "アミュレット" | "アミュレット・トークン"
            )
            || !self.keywords(id)?.contains("stack")
        {
            return Err(crate::EngineFailure::IllegalPlay(
                "Earth Rite requires an own field Stack amulet".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn finish_keyword_payment(
        &mut self,
        execution: &mut Execution,
        step: &CostStep,
    ) -> Result<()> {
        if step.keyword != Some(KeywordId::EarthRite) {
            return Ok(());
        }
        let Some(InstructionOutcome::Counter(change)) =
            execution.receipts.get(&step.instruction.receipt).cloned()
        else {
            return Err(invalid("missing Earth Rite counter result"));
        };
        let RuntimeHolder::Object { object } = change.holder else {
            return Err(invalid("Earth Rite counter holder"));
        };
        let removed = Nat(change
            .before
            .0
            .checked_sub(change.after.0)
            .ok_or_else(|| invalid("Earth Rite counter result"))?);
        if change.after == Nat(0) {
            self.internal_move_members(
                execution,
                from_ref(&object),
                "cemetery",
                ZoneKind::Cemetery,
                MoveMethod::Put,
                false,
            )?;
        }
        execution.facts.push(Fact::Payment {
            player: execution.context.controller,
            event: PaymentEvent {
                payment: PaymentResult {
                    complete: Bool(true),
                    steps: vec![RuntimeReceiptId {
                        execution: execution.id,
                        slot: step.instruction.receipt,
                    }],
                },
                component: PaidComponent::EarthRite {
                    paid: EarthRitePaidComponent {
                        removed_stack: removed,
                    },
                },
                phase: PaymentTime::PlayParameter,
                zero: Bool(removed == Nat(0)),
                replaced: Bool(false),
            },
        });
        Ok(())
    }
}
