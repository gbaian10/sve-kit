//! Summaries use returned debits, never reevaluated price expressions.

use super::super::{InstructionOutcome, Result, invalid, unsupported};
use super::{CostPaymentPurpose, Execution};
use crate::game::internal::fact_log::viewer;
use crate::game::player::observation::{PaymentKind, PaymentPurpose, ProcessingPayment};
use crate::ir::{Action, QuantityUnit};

fn kind(action: &Action) -> Result<PaymentKind> {
    Ok(match action {
        Action::Resource { .. } => PaymentKind::Pp,
        Action::Fusion { .. } => PaymentKind::Fusion,
        Action::ScheduleTurn { .. } => PaymentKind::SkipTurn,
        Action::Posture { .. } => PaymentKind::Posture,
        Action::Life { .. } => PaymentKind::Life,
        Action::Move { .. } => PaymentKind::Move,
        Action::Discard { .. } => PaymentKind::Discard,
        Action::Banish { .. } => PaymentKind::Banish,
        Action::Reveal { .. } => PaymentKind::Reveal,
        Action::FlipDown { .. } => PaymentKind::FlipDown,
        Action::FlipUp { .. } => PaymentKind::FlipUp,
        Action::Counter { kind, .. } => PaymentKind::Counter(kind.0.clone()),
        _ => return Err(unsupported("processing payment action")),
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PaymentBoundary {
    direct: usize,
    pp: usize,
    costs: usize,
    exchanges: usize,
}

impl PaymentBoundary {
    pub(super) const fn capture(execution: &Execution) -> Self {
        Self {
            direct: execution.processing_disclosure.direct_payments.len(),
            pp: execution.pp_payments.len(),
            costs: execution.cost_spec_payments.len(),
            exchanges: execution.exchange_payments.len(),
        }
    }
}

pub(super) fn completed(
    execution: &Execution,
    boundary: PaymentBoundary,
    since: PaymentBoundary,
) -> Result<Vec<ProcessingPayment>> {
    let mut payments: Vec<_> = execution
        .processing_disclosure
        .direct_payments
        .iter()
        .take(boundary.direct)
        .skip(since.direct)
        .cloned()
        .collect();
    // CostSpec and exchange PP are the same debits already recorded by this writer.
    for payment in execution
        .pp_payments
        .iter()
        .take(boundary.pp)
        .skip(since.pp)
    {
        payments.push(ProcessingPayment {
            actor: viewer(payment.player).ok_or_else(|| invalid("processing payer"))?,
            kind: PaymentKind::Pp,
            purpose: payment.purpose,
            amount: payment.amount.0,
            unit: QuantityUnit::Pp,
        });
    }
    for payment in execution
        .cost_spec_payments
        .iter()
        .take(boundary.costs)
        .skip(since.costs)
    {
        for step in &payment.steps {
            let kind = kind(&step.instruction.action)?;
            if kind == PaymentKind::Pp {
                continue;
            }
            payments.push(ProcessingPayment {
                actor: viewer(step.actor).ok_or_else(|| invalid("processing payer"))?,
                kind,
                purpose: Some(purpose(payment.purpose)),
                amount: step.quantity.amount.0,
                unit: step.quantity.unit,
            });
        }
    }
    for payment in execution
        .exchange_payments
        .iter()
        .take(boundary.exchanges)
        .skip(since.exchanges)
    {
        let Some(material) = &payment.material else {
            continue;
        };
        let (amount, unit) = exchange_quantity(&material.outcome)?;
        payments.push(ProcessingPayment {
            actor: viewer(payment.player).ok_or_else(|| invalid("processing exchange payer"))?,
            kind: kind(&material.instruction.action)?,
            purpose: Some(PaymentPurpose::Exchange),
            amount,
            unit,
        });
    }
    Ok(payments)
}

pub(super) const fn purpose(purpose: CostPaymentPurpose) -> PaymentPurpose {
    match purpose {
        CostPaymentPurpose::Original => PaymentPurpose::Original,
        CostPaymentPurpose::Modes => PaymentPurpose::Modes,
        CostPaymentPurpose::Additional { .. } => PaymentPurpose::Additional,
        CostPaymentPurpose::Optional { required, .. } => PaymentPurpose::Optional { required },
        CostPaymentPurpose::Resolution { .. } => PaymentPurpose::Resolution,
        CostPaymentPurpose::ResourceEntry => PaymentPurpose::ResourceEntry,
    }
}

fn exchange_quantity(outcome: &InstructionOutcome) -> Result<(i64, QuantityUnit)> {
    Ok(match outcome {
        InstructionOutcome::Payment { quantity, .. } => (quantity.amount.0, quantity.unit),
        InstructionOutcome::Discard(outcome) => (
            i64::try_from(outcome.moves.len()).map_err(invalid)?,
            QuantityUnit::Cards,
        ),
        InstructionOutcome::Move(outcome) => (
            i64::try_from(outcome.moved.len()).map_err(invalid)?,
            QuantityUnit::Cards,
        ),
        InstructionOutcome::Banish(outcome) => (
            i64::try_from(outcome.moves.len()).map_err(invalid)?,
            QuantityUnit::Cards,
        ),
        InstructionOutcome::Flip(outcome) => (
            i64::try_from(outcome.successful.len()).map_err(invalid)?,
            QuantityUnit::Cards,
        ),
        InstructionOutcome::Counter(outcome) => (
            outcome
                .before
                .0
                .checked_sub(outcome.after.0)
                .and_then(|amount| i64::try_from(amount).ok())
                .ok_or_else(|| invalid("processing counter debit"))?,
            QuantityUnit::Counters,
        ),
        _ => return Err(unsupported("processing exchange outcome")),
    })
}
