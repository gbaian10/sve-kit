//! Questions: what one seat must decide, with every legal answer domain made explicit.
//!
//! A question's purpose is its rules role, its use the specific procedure and its shape
//! the strict answer meaning. Sharing a shape never drops a constraint of the procedure.

mod selection;
mod shape;

pub use selection::{
    Bounds, Candidate, Capacity, CostTotal, Destination, Distinct, Edge, GroupAssignment, Origin,
    RoleSize, Selection, SelectionGroup, SelectionRole,
};
pub use shape::{
    AttackOption, AttackTarget, Bucket, ChoiceOption, ChoiceValue, DiceChoice, EarlierChoice,
    EmptyDistribution, EvolutionHint, EvolutionPayment, MainIntent, OrderDirection, OrderItem,
    QuantityUnit, QuestionShape, QuickAction, QuickOption, QuickWindow, ReplacementRoute,
    StepwiseChoice, TokenChoice, TriggerEvent, TriggerIcon,
};

use crate::answer::Answer;
use crate::common::{QuestionId, RuleRef, Seat};
use crate::number::Nat;
use crate::observation::ProcessingContext;
use alloc::borrow::Cow;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize, Serializer};

/// Rules role of a question, independent of its answer shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionPurpose {
    /// A parameter of a play, including a nested play.
    PlayParameter,
    /// A choice belonging to an effect's resolution.
    ResolutionChoice,
    /// A choice a rules procedure requires.
    RuleChoice,
    /// A Quick window, even when passing is the only answer.
    QuickWindow,
    /// The turn player's main-phase action window.
    MainAction,
}

/// Strict answer meaning shared by a question shape and its answer.
///
/// It names the tag both carry on the wire; it is not itself a wire value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    /// A set of candidates with groups and roles.
    Select,
    /// Listed options with typed values.
    Choose,
    /// Keep the hand or return all of it in order.
    Mulligan,
    /// A main-phase intent and its target.
    ActionWindow,
    /// A Quick action or an explicit pass.
    Quick,
    /// One listed evolution payment.
    EvolutionPayment,
    /// Entry resources and the EP substitution.
    ResourcePayment,
    /// Accept or decline.
    Confirm,
    /// A number within bounds.
    Integer,
    /// Keep or reroll a die.
    DiceReroll,
    /// A complete permutation.
    Order,
    /// Accept a Drive trigger and name its target.
    DriveTrigger,
    /// Every item placed in exactly one bucket.
    Partition,
    /// One share per recipient.
    Distribute,
}

/// The specific procedure a question belongs to.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum QuestionUse {
    /// Opening: place one start amulet.
    StartAmulet,
    /// Opening: choose who plays first.
    FirstPlayer,
    /// Opening: keep the hand or return all of it.
    Mulligan,
    /// Main phase: choose an intent.
    MainAction,
    /// A Quick window.
    Quick,
    /// Choose one effective ability.
    Ability,
    /// Evolution: choose the evolve card and its face.
    EvolutionForm,
    /// Evolution: choose the materials.
    EvolutionMaterial,
    /// Evolution: choose the hand cards to discard.
    EvolutionDiscard,
    /// Evolution: choose the PP, EP and SEP payment.
    EvolutionPayment,
    /// Pay intrinsic entry resources.
    ResourceEntryPayment,
    /// Decide the posture of a card entering with Ward.
    WardEntry,
    /// End phase: act any standing Guard followers.
    EndGuard,
    /// End phase: discard down to the hand limit.
    HandLimitDiscard,
    /// Declare whether to pay an optional cost.
    OptionalCost,
    /// Decide a payment during resolution.
    ResolutionPayment,
    /// Decide an optional effect.
    MayEffect,
    /// Continue or stop drawing.
    DrawContinue,
    /// Choose cost materials.
    CostMaterial,
    /// Discard as a cost.
    CostDiscard,
    /// Discard by an effect.
    EffectDiscard,
    /// Choose an exchange route.
    ExchangeRoute,
    /// Choose which pending occurrence resolves next.
    PendingOrder,
    /// Admit one simultaneous trigger under a quota.
    TriggerQuota,
    /// Declare X.
    DeclareX,
    /// Keep or reroll a die.
    DiceReroll,
    /// Order cards.
    OrderCards,
    /// Order damage replacements.
    DamageReplacementOrder,
    /// Accept a Drive trigger.
    DriveTrigger,
    /// Choose searched cards and their groups.
    Search,
    /// Assign found cards to a destination.
    SearchPartition,
    /// Choose which arrivals take the remaining space.
    MovementCapacity,
    /// Distribute damage.
    DamageDistribution,
    /// Keep the original search destination or the replacement.
    SearchReplacement,
    /// Declare targets.
    Targets,
    /// Select cards during resolution.
    ResolutionSelection,
    /// Choose which tokens fit the remaining space.
    TokenCapacity,
    /// Declare modes.
    Modes,
    /// Choose a resolution alternative.
    ResolutionAlternative,
}

impl QuestionUse {
    /// The only answer meaning this use may carry.
    #[must_use]
    pub const fn shape(self) -> ShapeKind {
        match self {
            Self::StartAmulet
            | Self::EvolutionMaterial
            | Self::EvolutionDiscard
            | Self::EndGuard
            | Self::HandLimitDiscard
            | Self::CostMaterial
            | Self::CostDiscard
            | Self::EffectDiscard
            | Self::Search
            | Self::MovementCapacity
            | Self::Targets
            | Self::ResolutionSelection => ShapeKind::Select,
            Self::FirstPlayer
            | Self::Ability
            | Self::EvolutionForm
            | Self::DrawContinue
            | Self::ExchangeRoute
            | Self::PendingOrder
            | Self::TriggerQuota
            | Self::SearchReplacement
            | Self::TokenCapacity
            | Self::Modes
            | Self::ResolutionAlternative => ShapeKind::Choose,
            Self::Mulligan => ShapeKind::Mulligan,
            Self::MainAction => ShapeKind::ActionWindow,
            Self::Quick => ShapeKind::Quick,
            Self::EvolutionPayment => ShapeKind::EvolutionPayment,
            Self::ResourceEntryPayment => ShapeKind::ResourcePayment,
            Self::WardEntry | Self::OptionalCost | Self::ResolutionPayment | Self::MayEffect => {
                ShapeKind::Confirm
            }
            Self::DeclareX => ShapeKind::Integer,
            Self::DiceReroll => ShapeKind::DiceReroll,
            Self::OrderCards | Self::DamageReplacementOrder => ShapeKind::Order,
            Self::DriveTrigger => ShapeKind::DriveTrigger,
            Self::SearchPartition => ShapeKind::Partition,
            Self::DamageDistribution => ShapeKind::Distribute,
        }
    }
}

/// Whether the host has proved that exactly one complete answer exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AutoAnswer {
    /// Exactly one complete answer exists, groups, roles, order and payment included.
    Unique {
        /// That answer.
        answer: Answer,
        /// Whether both seats can derive the uniqueness from public knowledge.
        public: bool,
    },
    /// More than one answer exists.
    NotUnique,
    /// Not proved either way; a first feasible answer is not a proof.
    Unknown,
}

/// The assistance slot of a question, fixed to `null` until assisted play is defined.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoAssistance;

impl Serialize for NoAssistance {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit()
    }
}

impl<'de> Deserialize<'de> for NoAssistance {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <()>::deserialize(deserializer).map(|()| Self)
    }
}

impl JsonSchema for NoAssistance {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("NoAssistance")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "null" })
    }
}

/// One question for one seat, without any authority continuation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Question {
    /// Code to answer with; it retires once the question is accepted, cancelled or rolled back.
    pub id: QuestionId,
    /// Seat that must answer, which need not be the controller.
    pub actor: Seat,
    /// Rules role.
    pub purpose: QuestionPurpose,
    /// Specific procedure.
    #[serde(rename = "use")]
    pub usage: QuestionUse,
    /// What is being processed.
    pub processing: ProcessingContext,
    /// Answer domain.
    pub shape: QuestionShape,
    /// Whether the answer is proved unique.
    pub auto_answer: AutoAnswer,
    /// Assistance slot.
    pub assistance: NoAssistance,
    /// Rules the question rests on.
    pub rules: Vec<RuleRef>,
}

impl Question {
    /// Whether the shape, and a proved unique answer, carry the meaning the use requires.
    #[must_use]
    pub fn is_coherent(&self) -> bool {
        let expected = self.usage.shape();
        let answer_matches = match &self.auto_answer {
            AutoAnswer::Unique { answer, .. } => answer.kind() == expected,
            AutoAnswer::NotUnique | AutoAnswer::Unknown => true,
        };
        self.shape.kind() == expected && answer_matches
    }
}

/// What a seat may know while another seat answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicWaiting {
    /// Public processing context; no candidate, uniqueness or private count.
    pub processing: Option<ProcessingContext>,
    /// Public die state while the other seat decides a reroll.
    pub dice: Option<PublicDice>,
}

/// A public die value and the rerolls left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublicDice {
    /// Current public value.
    pub value: Nat,
    /// Remaining rerolls.
    pub remaining: Nat,
}
