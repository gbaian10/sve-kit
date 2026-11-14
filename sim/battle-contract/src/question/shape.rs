//! Answer domains: one variant per strict answer meaning.

use super::ShapeKind;
use super::selection::{Bounds, Candidate, Capacity, Destination, Origin, Selection};
use crate::common::{CardRef, ObjectHandle, OptionId, Seat, SourceRef, Subject, TextRef, Zone};
use crate::number::{Int, Nat};
use crate::observation::{CostSummary, EvolutionState, Phase, Posture};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Fixed rules window in which Quick may be played.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuickWindow {
    /// After an attack is declared.
    Attack,
    /// During the end phase.
    EndPhase,
}

/// Revealed Drive trigger icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TriggerIcon {
    /// Critical.
    Critical,
    /// Draw.
    Draw,
    /// Stand.
    Stand,
    /// Heal.
    Heal,
    /// Front.
    Front,
}

/// Unit of a declared number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuantityUnit {
    /// Cards.
    Cards,
    /// Damage.
    Damage,
    /// Counters.
    Counters,
    /// Life.
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

/// Whether to go on with a repeated single-card operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StepwiseChoice {
    /// Perform the next operation.
    Continue,
    /// Finish without another operation.
    Stop,
}

/// Which destination found cards take.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReplacementRoute {
    /// The original instruction.
    Original,
    /// The replacement movement.
    Replacement,
}

/// What to do with a rolled die.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiceChoice {
    /// Accept the value.
    Keep,
    /// Roll again.
    Reroll,
}

/// How the listed order of an ordering answer is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrderDirection {
    /// The first item ends up topmost and the last one bottommost.
    TopDown,
    /// The first item applies first.
    Application,
}

/// What an empty set of recipients means for a distribution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmptyDistribution {
    /// No recipient: an empty answer is accepted without reaching the total.
    AllowWithoutTotal,
    /// No recipient: no answer is accepted.
    Reject,
}

/// Public summary of the completed event that made an occurrence pending.
///
/// Two occurrences of one source differ by these details, so they are typed here and
/// never read back from a description.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TriggerEvent {
    /// Drive was acquired.
    DriveGained,
    /// A race completed.
    Race {
        /// Which race of the turn it was.
        ordinal: Nat,
    },
    /// A card entered the field.
    EnterField {
        /// Rules names of the entering card.
        rules_names: Vec<String>,
        /// Zone it came from; absent when it was created.
        from: Option<Zone>,
    },
    /// A card entered EX.
    EnterEx,
    /// A stat changed.
    StatChange,
    /// A posture changed.
    Posture {
        /// Resulting posture.
        posture: Posture,
    },
    /// A card was destroyed.
    Destroy,
    /// The source itself left the field, by any route that is not one of the events here.
    LeaveField,
    /// Union Burst was activated.
    UnionBurst,
    /// Cards were fused while the source watched.
    Fusion,
    /// The source itself was used as a fusion material; it has no slot any more.
    FusedAsMaterial,
    /// A card was played.
    Play,
    /// Cards were discarded.
    Discard,
    /// Cards were banished.
    Banish,
    /// Cards were drawn.
    Draw,
    /// A Drive check completed.
    DriveChecked,
    /// A Drive trigger completed.
    DriveTrigger,
    /// A die value was accepted.
    Dice {
        /// Accepted value.
        value: Nat,
    },
    /// Equipment was attached.
    Equip,
    /// Cards moved between zones.
    Move {
        /// Player whose zone they left.
        player: Seat,
        /// Zone they left.
        from: Zone,
        /// Zone they entered.
        to: Zone,
    },
    /// A follower attacked.
    Attack {
        /// Which attack of the turn it was.
        ordinal: Nat,
    },
    /// A phase began.
    Phase {
        /// The phase.
        phase: Phase,
        /// Turn number.
        turn: Nat,
        /// Turn player.
        player: Seat,
    },
    /// A follower evolved.
    Evolve {
        /// Evolved or super evolved.
        state: EvolutionState,
    },
    /// Leader life changed.
    LifeChange {
        /// Signed change.
        delta: Int,
        /// Whose leader.
        player: Seat,
    },
    /// Damage was dealt.
    Damage {
        /// Amount dealt.
        amount: Nat,
    },
}

/// One token offered by a token-capacity option, before any token object exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TokenChoice {
    /// Rules name of the token.
    pub rules_name: String,
    /// Its face in the pinned snapshot, when known.
    pub card: Option<CardRef>,
}

/// Typed meaning of one option; never a free label or an index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChoiceValue {
    /// A seat.
    Seat {
        /// The seat.
        player: Seat,
    },
    /// One effective ability.
    Ability {
        /// Text supplying the ability and its current holder.
        source: SourceRef,
    },
    /// One evolve card and the face it would take.
    EvolutionForm {
        /// The evolve card.
        object: ObjectHandle,
        /// Destination face, which may differ from the current one.
        face: CardRef,
    },
    /// Continue or stop.
    Stepwise {
        /// The step.
        step: StepwiseChoice,
    },
    /// A genuine exchange route.
    Route {
        /// Native route index; gaps are kept.
        route: Nat,
    },
    /// A pending occurrence.
    Occurrence {
        /// Text that produced it.
        source: SourceRef,
        /// Its controller.
        controller: Seat,
        /// The completed event that produced it, with its public details.
        event: TriggerEvent,
        /// Slot of the source when that event completed, when the event records one.
        position: Option<Nat>,
    },
    /// A destination for found cards.
    Destination {
        /// Original or replacement route.
        route: ReplacementRoute,
        /// Where the cards go on that route.
        destination: Destination,
    },
    /// A complete combination of tokens.
    Tokens {
        /// The tokens created by this option.
        tokens: Vec<TokenChoice>,
    },
    /// A printed mode or resolution alternative.
    Mode {
        /// Printed mode identifier, never renumbered.
        mode: Nat,
        /// Printed text of the mode, when it has an exact correspondence.
        text: Option<TextRef>,
    },
}

/// One option of a choice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChoiceOption {
    /// Code to answer with.
    pub id: OptionId,
    /// Typed meaning.
    pub value: ChoiceValue,
    /// Cost summary of the option, when it has one.
    pub cost: Option<CostSummary>,
}

/// An earlier, already completed choice shown for context; it cannot be answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EarlierChoice {
    /// Seat that made the choice.
    pub actor: Seat,
    /// What was chosen.
    pub value: ChoiceValue,
}

/// An authorized attack recipient.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttackTarget {
    /// An opposing leader.
    Leader {
        /// The leader's seat.
        player: Seat,
    },
    /// An identifiable opposing field card.
    Card {
        /// Key of the card in the object table.
        object: ObjectHandle,
    },
}

/// One target of one attack intent; it is no target of any other attacker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttackOption {
    /// Code to answer with.
    pub id: OptionId,
    /// The recipient.
    pub target: AttackTarget,
}

/// Nullable hints for an evolve intent; they never replace the payment question.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvolutionHint {
    /// Whether super evolution is possible from here, when known.
    pub super_possible: Option<bool>,
    /// Whether paying with EP is possible from here, when known.
    pub ep_possible: Option<bool>,
}

/// One main-phase intent; its parameters follow in later questions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MainIntent {
    /// Start the card-play declaration.
    Play {
        /// Code to answer with.
        id: OptionId,
        /// Card to play, at its current permitted origin.
        source: ObjectHandle,
        /// Quote in the current context; payment prices it again.
        cost: Option<CostSummary>,
    },
    /// Start the declaration of one activated ability.
    Activate {
        /// Code to answer with.
        id: OptionId,
        /// Holder of the ability.
        holder: ObjectHandle,
        /// Text supplying the ability.
        ability: SourceRef,
    },
    /// Start normal evolution; its form and payment are asked next.
    Evolve {
        /// Code to answer with.
        id: OptionId,
        /// Field card that evolves.
        holder: ObjectHandle,
        /// Text supplying the evolution.
        ability: SourceRef,
        /// Optional hints.
        hint: Option<EvolutionHint>,
    },
    /// Declare one attack.
    Attack {
        /// Code to answer with.
        id: OptionId,
        /// Attacking card.
        attacker: ObjectHandle,
        /// Legal recipients of this attacker only.
        targets: Vec<AttackOption>,
    },
    /// End the main phase.
    EndPhase {
        /// Code to answer with.
        id: OptionId,
    },
}

/// Whether a Quick option plays a card or an ability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuickAction {
    /// Play the card.
    Play,
    /// Play an ability of the card.
    Ability,
}

/// One Quick action; passing is a separate answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuickOption {
    /// Code to answer with.
    pub id: OptionId,
    /// Card play or ability play.
    pub action: QuickAction,
    /// The card, or the exact ability text.
    pub source: SourceRef,
    /// Where the card is.
    pub origin: Option<Origin>,
}

/// One complete legal evolution payment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvolutionPayment {
    /// Play points paid.
    pub pp: Int,
    /// Whether one EP replaces one PP.
    pub use_ep: bool,
    /// Whether one SEP is paid for super evolution.
    pub super_evolution: bool,
}

/// One item of an ordering.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrderItem {
    /// Code to answer with.
    pub id: OptionId,
    /// Authorized card or provider.
    pub subject: Option<Subject>,
    /// Exact text of a replacement, which a card alone does not identify.
    pub source: Option<SourceRef>,
}

/// One bucket of a partition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartitionBucket {
    /// Printed position of the bucket, counted from zero.
    pub bucket: Nat,
    /// Where its items go; absent when they stay where they are.
    pub destination: Option<Destination>,
    /// Number of items it must receive.
    pub bounds: Bounds,
    /// Whether the order of its items matters.
    pub ordered: bool,
}

/// The answer domain of a question.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum QuestionShape {
    /// Select a set of candidates.
    Select {
        /// Candidates and constraints.
        selection: Selection,
    },
    /// Choose among typed options.
    Choose {
        /// The options, in their meaningful order.
        options: Vec<ChoiceOption>,
        /// How many options an answer takes.
        bounds: Bounds,
        /// Evaluated number of selections for a mode choice.
        count: Option<Nat>,
        /// Earlier public choices of the same simultaneous batch.
        earlier: Vec<EarlierChoice>,
        /// Space the chosen option must fit, when that is what is asked.
        capacity: Option<Capacity>,
    },
    /// Keep the opening hand or return all of it; there is no partial mulligan.
    Mulligan {
        /// The whole hand.
        hand: Vec<Candidate>,
    },
    /// Choose a main-phase intent.
    ActionWindow {
        /// Intents in public zone order, ending the phase last.
        intents: Vec<MainIntent>,
    },
    /// Play Quick or pass; passing is always legal.
    Quick {
        /// The rules window.
        window: QuickWindow,
        /// Available Quick actions, possibly none.
        options: Vec<QuickOption>,
    },
    /// Choose one complete evolution payment.
    EvolutionPayment {
        /// Every legal payment.
        payments: Vec<EvolutionPayment>,
    },
    /// Pay intrinsic entry resources.
    ResourcePayment {
        /// Resource candidates and constraints.
        materials: Selection,
        /// Whether one EP may replace one PP.
        ep_allowed: bool,
    },
    /// Accept or decline.
    Confirm {
        /// Legal answers.
        allowed: Vec<bool>,
        /// Card the decision is about, when it concerns one.
        subject: Option<Subject>,
        /// Where that card is.
        origin: Option<Origin>,
    },
    /// Declare a number.
    Integer {
        /// Effective bounds.
        bounds: Bounds,
        /// Unit of the number.
        unit: Option<QuantityUnit>,
        /// Text asking for it.
        source: Option<SourceRef>,
    },
    /// Keep or reroll a public die.
    DiceReroll {
        /// Current public value.
        value: Nat,
        /// Remaining rerolls.
        remaining: Nat,
        /// Legal answers.
        allowed: Vec<DiceChoice>,
    },
    /// Put every item in order.
    Order {
        /// Items to order.
        items: Vec<OrderItem>,
        /// How the listed order is read.
        direction: OrderDirection,
        /// Where the ordered cards go.
        destination: Option<Destination>,
    },
    /// Accept a revealed Drive trigger.
    DriveTrigger {
        /// Revealed icon.
        icon: TriggerIcon,
        /// Legal accept answers.
        allowed: Vec<bool>,
        /// Followers the icon may designate.
        targets: Vec<Candidate>,
        /// Number of targets an accepting answer names.
        target_bounds: Bounds,
    },
    /// Place every item of a frozen set in exactly one bucket.
    Partition {
        /// The complete frozen set.
        items: Vec<Candidate>,
        /// Every bucket, the one for unselected items included.
        buckets: Vec<PartitionBucket>,
    },
    /// Give each recipient a share.
    Distribute {
        /// Recipients, each answered exactly once.
        recipients: Vec<Candidate>,
        /// Sum the shares must reach.
        total: Nat,
        /// Bounds of each share.
        share: Bounds,
        /// Meaning of an empty recipient set.
        empty: EmptyDistribution,
    },
}

impl QuestionShape {
    /// Answer meaning of this domain.
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
