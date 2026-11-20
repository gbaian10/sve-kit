//! Processing information authorized by actual declaration and input producers.

use super::{CardFace, EffectSource};
use crate::game::player::{ObjectHandle, Viewer};
use crate::ir::QuantityUnit;

/// Producer-recorded progress, independent of a question's answer shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessingStage {
    /// Formal parameters are being declared.
    Declare,
    /// Formal targets are being chosen.
    Target,
    /// Amounts are being allocated.
    Allocate,
    /// Costs are being paid.
    Pay,
    /// An established play is resolving.
    Resolve,
}

/// A subject known at the declaration point; links never follow a new generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessingSubject {
    /// A leader has no physical object identity.
    Leader(Viewer),
    /// A frozen face with an optional uninterrupted current link.
    Card {
        /// Effective face when declared.
        card: CardFace,
        /// Current link, absent after losing the original generation or knowledge period.
        object: Option<ObjectHandle>,
    },
}

/// A procedure's actual kind, without its program coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcedureKind {
    /// Players act in the producer's order.
    EachPlayer,
    /// A body is executed repeatedly.
    Repeat,
}

/// Position authorized for the player currently participating in a procedure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessingProcedure {
    /// Runtime producer, not inferred from the question.
    pub kind: ProcedureKind,
    /// Procedure participant, which may differ from the question actor.
    pub actor: Viewer,
    /// One-based stage or iteration number.
    pub round: u64,
}

/// A declaration already public or known to this viewer; never a receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessingDeclaration {
    /// Modes in printed order.
    Modes {
        /// Actual declarer.
        actor: Viewer,
        /// Printed mode numbers.
        modes: Vec<u64>,
    },
    /// Explicitly chosen X.
    X {
        /// Actual declarer.
        actor: Viewer,
        /// Accepted value.
        value: u64,
    },
    /// A number an effect explicitly asks a player to declare.
    Number {
        /// Actual declarer.
        actor: Viewer,
        /// Accepted value.
        value: u64,
    },
    /// An accepted public declaration from the card catalogue.
    Name {
        /// Actual declarer.
        actor: Viewer,
        /// Accepted rules name.
        rules_name: String,
    },
    /// Formal targets in accepted order.
    Targets {
        /// Actual declarer.
        actor: Viewer,
        /// Every member must be authorized; an unknown member hides the declaration.
        targets: Vec<ProcessingSubject>,
    },
    /// Acceptance is distinct from subsequent payment.
    OptionalCost {
        /// Actual declarer.
        actor: Viewer,
        /// Whether declining cancels the play.
        required: bool,
        /// Accepted or declined.
        accepted: bool,
    },
    /// Formal damage assignment, before any damage is dealt.
    Distribution {
        /// Actual declarer.
        actor: Viewer,
        /// Authorized recipients and their accepted quantities.
        allocations: Vec<(ProcessingSubject, u64)>,
    },
}

/// Kind of a completed debit, independent of its quoted expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaymentKind {
    /// Play or ability PP.
    Pp,
    /// Act cards.
    Posture,
    /// Pay leader life.
    Life,
    /// Spend an EP on resource entry.
    Ep,
    /// Move intrinsic entry resources.
    EntryResources,
    /// Fuse cards as a cost.
    Fusion,
    /// Forgo the next turn.
    SkipTurn,
    /// Move cards.
    Move,
    /// Discard cards.
    Discard,
    /// Banish cards.
    Banish,
    /// Reveal cards.
    Reveal,
    /// Turn evolve-deck cards face down.
    FlipDown,
    /// Turn evolve-deck cards face up.
    FlipUp,
    /// Remove named counters.
    Counter(String),
}

/// Rules role of a completed payment, without its internal receipt identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaymentPurpose {
    /// Original card or ability costs.
    Original,
    /// Costs of chosen modes.
    Modes,
    /// Additional printed costs.
    Additional,
    /// A declared optional cost.
    Optional {
        /// Whether declining cancels play.
        required: bool,
    },
    /// A payment offered during resolution.
    Resolution,
    /// Intrinsic entry costs.
    ResourceEntry,
    /// Accepted exchange materials.
    Exchange,
}

/// One successful debit. Partial or quoted payments are not included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessingPayment {
    /// Actual payer.
    pub actor: Viewer,
    /// What was paid.
    pub kind: PaymentKind,
    /// Cost group, absent when the debit writer did not identify one.
    pub purpose: Option<PaymentPurpose>,
    /// Writer-returned amount, including a successful zero.
    pub amount: i64,
    /// Unit returned by the same writer.
    pub unit: QuantityUnit,
}

/// One disclosed formal execution, ordered from outer to inner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessingContext {
    /// Only the current question actor gets private progress; others retain public milestones.
    pub stage: Option<ProcessingStage>,
    /// Execution controller, not necessarily the participant or question actor.
    pub controller: Viewer,
    /// Actual question actor only when this is their current published frame.
    pub actor: Option<Viewer>,
    /// Independently authorized frozen text origin.
    pub source: Option<EffectSource>,
    /// Independently authorized declaration subject.
    pub subject: Option<ProcessingSubject>,
    /// Actual procedure position, only when authorized by participation.
    pub procedure: Option<ProcessingProcedure>,
    /// Already accepted declarations authorized to this viewer.
    pub declared: Vec<ProcessingDeclaration>,
    /// Completed debits; this does not assert that every cost of the play has completed.
    pub payments: Vec<ProcessingPayment>,
}
