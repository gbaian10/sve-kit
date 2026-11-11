//! Read-only cost summaries; payment revalidates the current authority.

use crate::game::player::ObjectHandle;

/// Context in which an amount was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteBasis {
    /// Before play; moving the card or later declarations can change the price.
    CurrentContext,
    /// A play is awaiting input before its atomic payment.
    BeforePayment,
}

/// The PP part of one cost group, without private expression bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PpQuote {
    /// Currently known amount.
    Known(i64),
    /// Fixed PP plus repeated payments of the same declared X.
    Variable {
        /// Known fixed portion.
        fixed: i64,
        /// Number of PP payments using X.
        copies: u64,
        /// Declared value, when already fixed.
        x: Option<u64>,
    },
    /// This context cannot safely provide an amount.
    Unavailable,
}

/// A cost component; these are summaries, not choices or payment receipts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CostLine {
    /// PP portion of this group.
    Pp(PpQuote),
    /// Act the source or selected materials.
    Posture {
        /// Known number of cards, when determined.
        cards: Option<u64>,
    },
    /// Signed leader life change.
    Life {
        /// Known signed change, when determined.
        change: Option<i64>,
    },
    /// Remove named counters.
    Counter {
        /// Counter rules name.
        name: String,
        /// Known number of counters, when determined.
        count: Option<u64>,
    },
    /// Move cards as payment.
    Move {
        /// Known number of cards, when determined.
        cards: Option<u64>,
    },
    /// Discard cards as payment.
    Discard {
        /// Known number of cards, when determined.
        cards: Option<u64>,
        /// Whether the particular cost materials have been fixed.
        selected: bool,
    },
    /// Banish cards as payment.
    Banish {
        /// Known number of cards, when determined.
        cards: Option<u64>,
    },
    /// Reveal selected cards as payment.
    Reveal {
        /// Known number of cards, when determined.
        cards: Option<u64>,
    },
    /// Fusion materials.
    Fusion {
        /// Known number of cards, when determined.
        cards: Option<u64>,
        /// Whether the particular cost materials have been fixed.
        selected: bool,
    },
    /// Intrinsic entry resources, possibly with EP substitution.
    EntryResources {
        /// Number of intrinsic resources required.
        cards: u64,
        /// Chosen EP substitution, absent before selection.
        use_ep: Option<bool>,
    },
    /// An additional rules prerequisite or unquoted component.
    Other,
}

/// Rules role of a cost group, without a receipt or declaration identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CostGroupKind {
    /// Original card or ability costs.
    Original,
    /// Additional costs printed on a card.
    Additional,
    /// Costs of selected modes.
    Mode,
    /// A declaration whose payment is separate from acceptance.
    Optional {
        /// Whether acceptance is required.
        required: bool,
        /// Current declaration, if answered.
        accepted: Option<bool>,
    },
}

/// One set of costs with an explicit declaration status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostGroup {
    /// Public rules role.
    pub kind: CostGroupKind,
    /// Known components, including non-PP costs.
    pub lines: Vec<CostLine>,
}

/// Conditions still needed before a complete cost can be stated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    /// Intrinsic resources or their EP substitution remain undecided.
    EntryResources,
    /// A component is not safely quotable in this context.
    Unavailable,
}

/// An actor's cost summary; no amount here claims that payment has committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostSummary {
    /// Exact context of the quote.
    pub basis: QuoteBasis,
    /// PP total only when every cost component and declaration is determined.
    pub total_pp: Option<i64>,
    /// Known portions and declaration status.
    pub groups: Vec<CostGroup>,
    /// Remaining cost conditions.
    pub pending: Vec<PendingCost>,
}

/// One of the viewer's own identifiable cards before play.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardCostQuote {
    /// Viewer-authorized card reference.
    pub card: ObjectHandle,
    /// Current-context estimate, repriced by ordinary payment.
    pub cost: CostSummary,
}

/// A play's costs are visible only to its controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayCostQuote {
    /// Identifiable holder, when still authorized.
    pub source: Option<ObjectHandle>,
    /// Current declaration and payment state.
    pub cost: CostSummary,
}

/// Cost information authorized for one perspective.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CostObservation {
    /// Only the viewer's own loaded Hand and EX cards.
    pub cards: Vec<CardCostQuote>,
    /// Current play, only for its controller.
    pub current_play: Option<PlayCostQuote>,
}
