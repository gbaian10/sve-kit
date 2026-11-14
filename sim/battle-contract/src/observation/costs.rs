//! Cost summaries: quotes, never choices, receipts or a promise that payment is legal.

use crate::common::ObjectHandle;
use crate::number::{Int, Nat};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Context in which an amount was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuoteBasis {
    /// Before play; moving the card or later declarations can change the price.
    CurrentContext,
    /// A play is awaiting input before its atomic payment.
    BeforePayment,
}

/// The PP part of one cost group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PpQuote {
    /// A known amount.
    Known {
        /// Amount of PP.
        amount: Int,
    },
    /// Fixed PP plus repeated payments of one declared X.
    Variable {
        /// Known fixed portion.
        fixed: Int,
        /// Number of payments that use X.
        copies: Nat,
        /// Declared value, once fixed.
        x: Option<Nat>,
    },
    /// This context cannot safely provide an amount; it is not zero.
    Unavailable,
}

/// One component of a cost group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CostLine {
    /// PP portion of the group.
    Pp {
        /// Quoted PP.
        quote: PpQuote,
    },
    /// Act the source or selected materials.
    Posture {
        /// Number of cards, once determined.
        cards: Option<Nat>,
    },
    /// Signed leader life change.
    Life {
        /// Signed change, once determined.
        change: Option<Int>,
    },
    /// Remove named counters.
    Counter {
        /// Rules name of the counter.
        name: String,
        /// Number of counters, once determined.
        count: Option<Nat>,
    },
    /// Move cards as payment.
    Move {
        /// Number of cards, once determined.
        cards: Option<Nat>,
    },
    /// Discard cards as payment.
    Discard {
        /// Number of cards, once determined.
        cards: Option<Nat>,
        /// Whether the particular materials are fixed.
        selected: bool,
    },
    /// Banish cards as payment.
    Banish {
        /// Number of cards, once determined.
        cards: Option<Nat>,
    },
    /// Reveal cards as payment.
    Reveal {
        /// Number of cards, once determined.
        cards: Option<Nat>,
    },
    /// Fusion materials.
    Fusion {
        /// Number of cards, once determined.
        cards: Option<Nat>,
        /// Whether the particular materials are fixed.
        selected: bool,
    },
    /// Intrinsic entry resources, possibly with one EP replacing one PP.
    EntryResources {
        /// Number of resources required.
        cards: Nat,
        /// Chosen EP substitution; absent before the choice.
        use_ep: Option<bool>,
    },
    /// An additional rules prerequisite or a component without a quote.
    Other,
}

/// Rules role of a cost group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CostGroupRole {
    /// Original card or ability costs.
    Original,
    /// Additional printed costs.
    Additional,
    /// Costs of selected modes.
    Mode,
    /// A cost whose declaration is separate from its payment.
    Optional {
        /// Whether acceptance is required.
        required: bool,
        /// The declaration, once answered; a declaration is not a payment receipt.
        accepted: Option<bool>,
    },
}

/// One set of costs with its declaration status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CostGroup {
    /// Rules role.
    pub role: CostGroupRole,
    /// Known components, including costs other than PP.
    pub lines: Vec<CostLine>,
}

/// A condition still needed before a complete cost can be stated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PendingCost {
    /// X has not been declared.
    X,
    /// A payment declaration has not been accepted or declined.
    Optional,
    /// Modes have not been chosen.
    Modes,
    /// Cost materials have not been chosen.
    Materials,
    /// An exchange declaration or route is undecided.
    Exchange,
    /// Intrinsic resources or their EP substitution are undecided.
    EntryResources,
    /// A component cannot safely be quoted in this context.
    Unavailable,
}

/// A cost quote; no amount here claims that payment has committed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CostSummary {
    /// Context of the quote.
    pub basis: QuoteBasis,
    /// PP total, only when every component and declaration is determined.
    pub total_pp: Option<Int>,
    /// Known portions and their declaration status.
    pub groups: Vec<CostGroup>,
    /// Conditions still undecided.
    pub pending: Vec<PendingCost>,
}

/// Quote for one of the viewer's own identifiable cards before play.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CardCostQuote {
    /// Key of the card in the object table.
    pub object: ObjectHandle,
    /// Estimate in the current context; payment prices it again.
    pub cost: CostSummary,
}

/// Costs of the current play, visible only to its controller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayCostQuote {
    /// Holder of the play, while still identifiable.
    pub source: Option<ObjectHandle>,
    /// Current declaration and payment state.
    pub cost: CostSummary,
}

/// Cost information authorized for one seat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CostObservation {
    /// Quotes for the viewer's own hand and EX cards.
    pub cards: Vec<CardCostQuote>,
    /// The current play, only for its controller.
    pub current_play: Option<PlayCostQuote>,
}
