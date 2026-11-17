//! Exchange writers retain actual debits independently of `CostSpec` invocations.

use super::super::InstructionOutcome;
use crate::ir::{Instruction, PlayerId, Quantity, ReceiptId};

/// One successfully paid material instruction of a play-cost exchange.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExchangeMaterialPayment {
    /// Frozen admitted instruction; its expressions are not actual paid quantities.
    pub instruction: Box<Instruction>,
    /// The writer's actual result, including physical generations and movement results.
    pub outcome: InstructionOutcome,
}

/// A completed exchange payment path, including intrinsic PP when the exchange was declined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExchangePayment {
    /// Actual payer.
    pub player: PlayerId,
    /// Exchange definition identity in this execution.
    pub receipt: ReceiptId,
    /// Whether the optional exchange was accepted, not whether intrinsic payment completed.
    pub accepted: bool,
    /// Actual PP debit after adjustments, including zero.
    /// This is the same debit recorded in `PpPayment`; consumers must not count it twice.
    pub pp: Quantity,
    /// Successfully paid material, absent for unaccepted exchanges or PP-only increases.
    pub material: Option<ExchangeMaterialPayment>,
}
