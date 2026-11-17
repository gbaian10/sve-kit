//! Completed cost facts stay separate from quotes and from whole-play completion.

use super::Execution;
use crate::ir::{Characteristics, Instruction, ObjectRef, PlayerId, Quantity, ReceiptId};
use alloc::vec::Vec;

/// The actual caller of a completed `CostSpec`, without inferring a purpose from its contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostPaymentPurpose {
    /// Original card or ability costs through the shared `CostSpec` path.
    Original,
    /// Costs of selected modes.
    Modes,
    /// An additional printed cost declaration.
    Additional {
        /// Declaration identity within this execution.
        receipt: ReceiptId,
    },
    /// A play-time cost whose acceptance was separate from payment.
    Optional {
        /// Declaration identity within this execution.
        receipt: ReceiptId,
        /// Whether declining would have cancelled the play.
        required: bool,
    },
    /// A payment offered during resolution.
    Resolution {
        /// Identity of the payment's completion result.
        receipt: ReceiptId,
    },
    /// The `CostSpec` part of resource entry; other direct resource payments are not included.
    ResourceEntry,
}

/// One successful instruction of a completed `CostSpec`, frozen before the frame disappears.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostStepPayment {
    /// Actor evaluated before paying this instruction.
    pub actor: PlayerId,
    /// The admitted source and action; its expressions must not be reevaluated as actual amounts.
    pub instruction: Box<Instruction>,
    /// Actual returned quantity, including a successfully paid zero.
    pub quantity: Quantity,
    /// Physical generations used for payment, even if they subsequently moved.
    pub subjects: Vec<ObjectRef>,
    /// Frozen characteristics already returned by the payment writer.
    pub characters: Vec<Characteristics>,
}

/// One fully paid `CostSpec` invocation, not proof that every cost of a play is captured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostSpecPayment {
    /// Explicit runtime producer, independent of step kinds.
    pub purpose: CostPaymentPurpose,
    /// Ordered successful steps; partial invocations are never appended.
    pub steps: Vec<CostStepPayment>,
}

pub(in crate::game::internal) fn record_cost_spec_payment(
    execution: &mut Execution,
    purpose: CostPaymentPurpose,
    steps: Vec<CostStepPayment>,
) {
    execution
        .cost_spec_payments
        .push(CostSpecPayment { purpose, steps });
}
