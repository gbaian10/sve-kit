//! Actual returned debits stay separate from cost quotes and declarations.

use crate::{Int, Seat};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a completed debit consumed or changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PaymentKind {
    /// Play points.
    Pp,
    /// Evolution points used for entry.
    Ep,
    /// Intrinsic entry cards.
    EntryResources,
    /// Fusion materials.
    Fusion,
    /// Forgo the next turn.
    SkipTurn,
    /// Act cards.
    Posture,
    /// Leader life.
    Life,
    /// Move cards.
    Move,
    /// Discard cards.
    Discard,
    /// Banish cards.
    Banish,
    /// Reveal cards.
    Reveal,
    /// Turn cards face down.
    FlipDown,
    /// Turn cards face up.
    FlipUp,
    /// Remove named counters.
    Counter {
        /// Rules name of the counter.
        name: String,
    },
}

/// Rules role of a completed debit, without receipt identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PaymentPurpose {
    /// Original card or ability costs.
    Original,
    /// Selected mode costs.
    Modes,
    /// Additional printed costs.
    Additional,
    /// A declared optional cost.
    Optional {
        /// Whether declining cancels the play.
        required: bool,
    },
    /// A payment offered during resolution.
    Resolution,
    /// Intrinsic entry costs.
    ResourceEntry,
    /// Accepted exchange costs.
    Exchange,
}

/// Unit returned by the actual payment writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaymentUnit {
    /// Cards.
    Cards,
    /// Damage.
    Damage,
    /// Counters.
    Counters,
    /// Leader life.
    Life,
    /// Play points.
    Pp,
    /// Evolution points.
    Ep,
    /// Super evolution points.
    Sep,
    /// Repetitions.
    Times,
}

/// A successful debit; no quantity asserts that all costs of the play have completed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessingPayment {
    /// Actual payer.
    pub actor: Seat,
    /// What was paid.
    pub kind: PaymentKind,
    /// Cost group, absent when the writer did not provide it.
    pub purpose: Option<PaymentPurpose>,
    /// Actual returned quantity, including a successful zero.
    pub amount: Int,
    /// Quantity unit.
    pub unit: PaymentUnit,
}
