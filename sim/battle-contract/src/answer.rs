//! Answers and the single command a seat submits at a time.

use crate::common::{OptionId, QuestionId, RequestId};
use crate::number::{Int, Nat};
use crate::question::{DiceChoice, ShapeKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The part of a selection assigned to one group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroupChoice {
    /// Printed position of the group.
    pub group: Nat,
    /// Candidates assigned to it, possibly none.
    pub selected: Vec<OptionId>,
}

/// The part of a selection given one role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoleChoice {
    /// Printed identifier of the role.
    pub role: Nat,
    /// Candidates given the role, possibly none.
    pub selected: Vec<OptionId>,
}

/// The items placed in one bucket.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BucketChoice {
    /// Printed position of the bucket.
    pub bucket: Nat,
    /// Its items, in order when the bucket is ordered.
    pub items: Vec<OptionId>,
}

/// The share given to one recipient.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Share {
    /// The recipient.
    pub option: OptionId,
    /// Its share.
    pub amount: Nat,
}

/// A complete answer; its tag must equal the tag of the question's shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Answer {
    /// The complete selected set.
    Select {
        /// Selected candidates.
        selected: Vec<OptionId>,
        /// Every group, when the question asks for an explicit assignment.
        groups: Option<Vec<GroupChoice>>,
        /// Every role the question lists.
        roles: Vec<RoleChoice>,
    },
    /// The chosen options.
    Choose {
        /// Chosen options; printed order, not this order, decides how modes resolve.
        chosen: Vec<OptionId>,
    },
    /// Keep the hand, or return all of it.
    Mulligan {
        /// Whether to return the hand and draw again.
        redraw: bool,
        /// The whole hand from top to bottom when redrawing, otherwise empty.
        bottom_order: Vec<OptionId>,
    },
    /// One intent and, for an attack, one of its targets.
    ActionWindow {
        /// Chosen intent.
        intent: OptionId,
        /// Target of that attack intent; absent for any other intent.
        target: Option<OptionId>,
    },
    /// One Quick action, or a pass.
    Quick {
        /// Chosen action; absent to pass.
        option: Option<OptionId>,
    },
    /// One of the listed evolution payments.
    EvolutionPayment {
        /// Play points paid.
        pp: Int,
        /// Whether one EP replaces one PP.
        use_ep: bool,
        /// Whether one SEP is paid for super evolution.
        super_evolution: bool,
    },
    /// Entry resources.
    ResourcePayment {
        /// Chosen resources.
        materials: Vec<OptionId>,
        /// Whether one EP replaces one PP.
        use_ep: bool,
    },
    /// Accept or decline.
    Confirm {
        /// The decision.
        accept: bool,
    },
    /// A declared number.
    Integer {
        /// The number.
        value: Nat,
    },
    /// Keep or reroll.
    DiceReroll {
        /// The decision.
        choice: DiceChoice,
    },
    /// A complete permutation.
    Order {
        /// Every item exactly once.
        order: Vec<OptionId>,
    },
    /// Accept or decline a Drive trigger.
    DriveTrigger {
        /// The decision.
        accept: bool,
        /// Designated follower; absent when declining or when none is needed.
        target: Option<OptionId>,
    },
    /// Every bucket with its items.
    Partition {
        /// Every bucket, empty ones included; each item appears exactly once.
        buckets: Vec<BucketChoice>,
    },
    /// One share per recipient.
    Distribute {
        /// Every recipient exactly once.
        shares: Vec<Share>,
    },
}

impl Answer {
    /// Answer meaning of this answer.
    #[must_use]
    pub const fn kind(&self) -> ShapeKind {
        match self {
            Self::Select { .. } => ShapeKind::Select,
            Self::Choose { .. } => ShapeKind::Choose,
            Self::Mulligan { .. } => ShapeKind::Mulligan,
            Self::ActionWindow { .. } => ShapeKind::ActionWindow,
            Self::Quick { .. } => ShapeKind::Quick,
            Self::EvolutionPayment { .. } => ShapeKind::EvolutionPayment,
            Self::ResourcePayment { .. } => ShapeKind::ResourcePayment,
            Self::Confirm { .. } => ShapeKind::Confirm,
            Self::Integer { .. } => ShapeKind::Integer,
            Self::DiceReroll { .. } => ShapeKind::DiceReroll,
            Self::Order { .. } => ShapeKind::Order,
            Self::DriveTrigger { .. } => ShapeKind::DriveTrigger,
            Self::Partition { .. } => ShapeKind::Partition,
            Self::Distribute { .. } => ShapeKind::Distribute,
        }
    }
}

/// The rules command of a submission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Answer the current question.
    Answer {
        /// Question being answered.
        question_id: QuestionId,
        /// The answer.
        answer: Answer,
    },
    /// Concede; legal while waiting and bound to no question.
    Concede,
}

/// One submission; a seat sends one rules command at a time and never retries it itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Submit {
    /// Label echoed by the receipt.
    pub request_id: RequestId,
    /// The command.
    pub command: Command,
}

/// Everything a client sends to the host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClientMessage {
    /// A rules command.
    #[schemars(title = "Submit")]
    Submit(Submit),
}
