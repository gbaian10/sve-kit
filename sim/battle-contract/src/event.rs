//! Committed events: what happened, for history, animation and sound.
//!
//! The board is always redrawn from the observation; no client replays rules from events.
//! Every field was authorized when the event occurred and is frozen as it was then.

use crate::common::{
    BatchId, CardRef, CauseId, EventId, ObjectHandle, PresentationId, RuleRef, Seat, SourceRef,
    Subject, Zone,
};
use crate::number::{Int, Nat};
use crate::observation::{
    AbilityKind, Declared, EvolutionState, Facing, Posture, RelationshipKind, TriggerTiming,
    TurnView,
};
use crate::question::TriggerIcon;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How a card physically moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MovementMethod {
    /// Put.
    Put,
    /// Returned.
    Return,
    /// Milled.
    Mill,
    /// Drawn.
    Draw,
    /// Discarded.
    Discard,
    /// Destroyed.
    Destroy,
    /// Banished.
    Banish,
    /// Played.
    Play,
    /// An ordinary procedure; its cause says which.
    Procedure,
}

/// A card as it was known when an event occurred; it never links to a later object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventCard {
    /// Face authorized at the event.
    pub card: CardRef,
    /// Object at the event, for linking the animation only.
    pub object: Option<ObjectHandle>,
}

/// One end of a movement; an unknown draw has no card here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    /// Player whose zone it is.
    pub player: Seat,
    /// The zone; absent outside the game, for creation or erasure.
    pub zone: Option<Zone>,
    /// Authorized offset.
    pub position: Option<Nat>,
    /// Authorized identity at this end.
    pub card: Option<EventCard>,
}

/// Rules class of dealt damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DamageClass {
    /// Ability damage.
    Ability,
    /// Attack damage outside combat.
    Attack,
    /// Combat damage.
    Combat,
    /// Attack damage dealt in combat.
    AttackCombat,
}

/// A numeric characteristic of a follower.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Stat {
    /// Attack.
    Attack,
    /// Life.
    Life,
}

/// One explicit numeric result, frozen when its action completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NumberChange {
    /// Value before, when the producer provides it.
    pub before: Option<Int>,
    /// Value after, when the producer provides it.
    pub after: Option<Int>,
    /// Actual signed change.
    pub delta: Int,
}

/// One changed stat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatChange {
    /// Which stat.
    pub stat: Stat,
    /// Its change.
    pub change: NumberChange,
}

/// A public player resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Resource {
    /// Play points.
    Pp,
    /// Maximum play points.
    MaxPp,
    /// Evolution points.
    Ep,
    /// Super evolution points.
    Sep,
    /// Leader life, when a rule raises it as a resource.
    Life,
}

/// Whether an evolution came from the rules action or from an effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionRole {
    /// Normal evolution.
    Normal,
    /// Evolution performed by an effect.
    Effect,
}

/// Whether a link was made or removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkChange {
    /// The link was made.
    Attached,
    /// The link was removed.
    Detached,
}

/// How an ability qualified for its play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AbilityQualification {
    /// An ordinary play.
    Ordinary,
    /// Fusion.
    Fusion,
    /// Union Burst.
    UnionBurst,
}

/// Rules point of a payment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaymentTime {
    /// While declaring a play.
    PlayParameter,
    /// During resolution.
    ResolutionChoice,
}

/// The public part of a completed payment; never a receipt or the hidden materials.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PaymentComponent {
    /// An ordinary payment with no keyword component of its own.
    General,
    /// An Earth Rite payment.
    EarthRite {
        /// Rules name of the keyword.
        keyword: String,
        /// Number of stacks removed.
        stacks: Nat,
    },
}

/// One zone of one player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneRef {
    /// Owner of the zone.
    pub player: Seat,
    /// The zone.
    pub zone: Zone,
}

/// Why a game ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResultReason {
    /// A player was defeated by the rules.
    Defeat,
    /// Both players were defeated at once.
    SimultaneousDefeat,
    /// An effect ended the game.
    Effect,
    /// A player conceded.
    Concession,
}

/// Why the rules cancelled a pending occurrence; unrelated to cancelling a presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PendingCancelReason {
    /// A required cost could not be paid.
    UnpaidCost,
    /// Another rule made it no longer applicable.
    Rule,
}

/// What happened; the tag fixes the fields.
///
/// A card taking part is `null` when this viewer was not authorized to identify it:
/// the event still happened, and no placeholder identity stands in for the card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventBody {
    /// One physical move; a draw or discard is this same event.
    Movement {
        /// How it moved.
        method: MovementMethod,
        /// Whether an effect instruction moved it, rather than a rule or procedure.
        by_effect: bool,
        /// Authorized end before the move.
        before: Endpoint,
        /// Authorized end after the move.
        after: Endpoint,
    },
    /// Cards were disclosed.
    Revealed {
        /// The disclosed cards.
        cards: Vec<EventCard>,
    },
    /// A card turned over.
    FacingChanged {
        /// The card.
        subject: Option<Subject>,
        /// Side now facing up.
        facing: Facing,
    },
    /// A card play was established; payment is complete.
    CardPlayed {
        /// Seat that played it.
        player: Seat,
        /// The card, when authorized.
        card: Option<EventCard>,
    },
    /// A played card finished resolving.
    CardResolved {
        /// Seat that played it.
        player: Seat,
        /// The card, when authorized.
        card: Option<EventCard>,
    },
    /// An ability play was established.
    AbilityPlayed {
        /// Seat that played it.
        player: Seat,
        /// Card holding the ability, when authorized.
        holder: Option<Subject>,
        /// The ability's text, when authorized.
        source: Option<SourceRef>,
        /// Category of the ability, when the producer provides it.
        ability: Option<AbilityKind>,
        /// How the play qualified; this is not the category.
        qualification: AbilityQualification,
    },
    /// A played ability finished resolving.
    AbilityResolved {
        /// Seat that played it.
        player: Seat,
        /// Card holding the ability, when authorized.
        holder: Option<Subject>,
        /// The ability's text, when authorized.
        source: Option<SourceRef>,
        /// Category of the ability, when the producer provides it.
        ability: Option<AbilityKind>,
    },
    /// An attack was declared.
    AttackDeclared {
        /// Attacking card.
        attacker: Option<Subject>,
        /// Leader or card attacked.
        target: Option<Subject>,
    },
    /// Damage was dealt; zero damage is not a hit.
    Damage {
        /// Authorized source.
        source: Option<Subject>,
        /// Leader or card damaged.
        target: Option<Subject>,
        /// Rules class.
        class: DamageClass,
        /// Actual amount.
        amount: Nat,
        /// Life of the target once this damage was applied.
        life_after: Option<Int>,
    },
    /// Leader life changed without damage.
    LifeChanged {
        /// Whose leader.
        player: Seat,
        /// The change.
        change: NumberChange,
    },
    /// Follower stats changed.
    StatsChanged {
        /// The follower.
        subject: Option<Subject>,
        /// Every changed stat.
        changes: Vec<StatChange>,
    },
    /// A player resource changed.
    ResourceChanged {
        /// Whose resource.
        player: Seat,
        /// Which resource.
        resource: Resource,
        /// The change.
        change: NumberChange,
    },
    /// Counters on a card changed.
    CounterChanged {
        /// The card.
        subject: Option<Subject>,
        /// Rules name of the counter.
        name: String,
        /// The change.
        change: NumberChange,
    },
    /// A card acted or stood.
    PostureChanged {
        /// The card.
        subject: Option<Subject>,
        /// Resulting posture.
        posture: Posture,
    },
    /// A destruction was attempted; only a movement shows that it succeeded.
    DestroyAttempted {
        /// The card.
        subject: Option<Subject>,
    },
    /// A banish was attempted; only a movement shows that it succeeded.
    BanishAttempted {
        /// The card.
        subject: Option<Subject>,
    },
    /// A replacement effect applied.
    ReplacementApplied {
        /// What it applied to, when authorized.
        subject: Option<Subject>,
        /// The replacement's text, when authorized.
        source: Option<SourceRef>,
    },
    /// A card left the game.
    Erased {
        /// The card.
        subject: Option<Subject>,
    },
    /// A follower evolved.
    Evolved {
        /// The follower.
        subject: Option<Subject>,
        /// The evolve card placed on it, when authorized.
        form: Option<EventCard>,
        /// Evolved or super evolved.
        state: EvolutionState,
        /// Rules action or effect.
        role: EvolutionRole,
    },
    /// Equipment was attached.
    Equipped {
        /// The equipment.
        equipment: Option<Subject>,
        /// Card bearing it.
        bearer: Option<Subject>,
    },
    /// A typed link between two cards changed.
    RelationshipChanged {
        /// Kind of link.
        relationship: RelationshipKind,
        /// Attached card.
        from: Option<Subject>,
        /// Card it is attached to.
        to: Option<Subject>,
        /// Made or removed.
        change: LinkChange,
    },
    /// A Fusion completed.
    Fusion {
        /// Controller of the card that fused.
        player: Seat,
        /// Card that fused.
        subject: Option<Subject>,
        /// Number of materials; the traits below are a set, not one entry per material.
        materials: Nat,
        /// Public traits of its materials.
        material_traits: Vec<String>,
    },
    /// A race completed.
    Race {
        /// Racing player.
        player: Seat,
        /// Card that raced.
        subject: Option<Subject>,
        /// Which race of the turn it was.
        ordinal: Nat,
    },
    /// Drive was acquired for the first time.
    DriveGained {
        /// Player gaining it.
        player: Seat,
        /// Card that gained it.
        subject: Option<Subject>,
    },
    /// A Drive trigger was accepted.
    DriveTrigger {
        /// Player whose check revealed it.
        player: Seat,
        /// The checked card, when authorized.
        card: Option<EventCard>,
        /// Accepted icon.
        icon: TriggerIcon,
    },
    /// A Drive check completed.
    DriveChecked {
        /// Checking player.
        player: Seat,
        /// The checked card, when authorized.
        card: Option<EventCard>,
        /// Whether its trigger was accepted.
        accepted: bool,
        /// Where the checked card went.
        destination: ZoneRef,
    },
    /// A payment completed.
    Payment {
        /// Paying player.
        player: Seat,
        /// Rules point of the payment.
        time: PaymentTime,
        /// Whether it was paid in full.
        complete: bool,
        /// Whether nothing had to be paid.
        zero: bool,
        /// Whether a replacement changed it.
        replaced: bool,
        /// What was paid, as far as it is public.
        component: PaymentComponent,
    },
    /// A die value was accepted; rerolled values are no events.
    Dice {
        /// Rolling player.
        player: Seat,
        /// Accepted value.
        value: Nat,
    },
    /// A zone was shuffled; no order is disclosed.
    Shuffled {
        /// Owner of the zone.
        player: Seat,
        /// The zone.
        zone: Zone,
    },
    /// Control of a card changed; its owner did not.
    ControlChanged {
        /// The card.
        subject: Option<Subject>,
        /// Its owner.
        owner: Seat,
        /// Controller before.
        before: Seat,
        /// Controller after.
        after: Seat,
    },
    /// A phase began, before its automatic actions.
    PhaseStarted {
        /// Public turn state on entry.
        turn: TurnView,
    },
    /// A turn began.
    TurnStarted {
        /// Public turn state on entry.
        turn: TurnView,
    },
    /// A scheduled turn was skipped.
    TurnSkipped {
        /// Player whose turn was skipped.
        player: Seat,
        /// Whether it was an additional turn.
        extra: bool,
    },
    /// The game ended.
    Result {
        /// Winner; absent for a draw.
        winner: Option<Seat>,
        /// Loser, when the source names one.
        loser: Option<Seat>,
        /// Why it ended.
        reason: ResultReason,
        /// Rule that decided it, when the source provides one.
        rule: Option<RuleRef>,
    },
    /// The game started.
    GameStarted {
        /// Player who goes first.
        first_player: Seat,
    },
    /// A player finished the mulligan decision.
    Mulligan {
        /// The player.
        player: Seat,
        /// Number of cards returned and redrawn; zero when the hand was kept.
        redrawn: Nat,
    },
    /// A choice became public.
    ChoiceDeclared {
        /// Declaring player.
        player: Seat,
        /// The public declaration.
        declared: Declared,
    },
    /// Targets became public.
    TargetsDeclared {
        /// Declaring player.
        player: Seat,
        /// Text that targets, when authorized.
        source: Option<SourceRef>,
        /// The public targets.
        targets: Vec<Subject>,
    },
    /// A trigger became pending.
    TriggerPending {
        /// Its controller.
        controller: Seat,
        /// Its text.
        source: SourceRef,
        /// Completed event category that produced it, when known.
        timing: Option<TriggerTiming>,
    },
    /// The rules cancelled a pending occurrence.
    PendingCancelled {
        /// Its text.
        source: SourceRef,
        /// Why it was cancelled.
        reason: PendingCancelReason,
    },
}

/// One committed event in this viewer's own numbering.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BattleEvent {
    /// Viewer-local identity.
    pub id: EventId,
    /// Viewer-local cause; a shared cause does not mean simultaneity.
    pub cause: CauseId,
    /// An already public causal parent.
    pub parent: Option<EventId>,
    /// What happened.
    pub body: EventBody,
    /// Authorized subjects for generic highlighting.
    pub about: Vec<Subject>,
    /// The provisional event this one promotes, when it was shown early.
    pub presentation_id: Option<PresentationId>,
}

/// One complete simultaneous batch, containing only what this viewer may see.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EventBatch {
    /// Identity of the batch.
    pub id: BatchId,
    /// Its events; a batch is never split.
    pub events: Vec<BattleEvent>,
}

/// Why committed events are not delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventGap {
    /// Some source could not be projected completely and safely for this viewer.
    ProjectionGap,
}

/// The committed events of one update.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventDelivery {
    /// Every complete batch not delivered before; empty in the first update.
    Available {
        /// New batches in order.
        batches: Vec<EventBatch>,
        /// Batches of this update to play now, each as a whole.
        animate: Vec<BatchId>,
    },
    /// Events stopped for this viewer; the board stays safe to use.
    Unavailable {
        /// Why.
        reason: EventGap,
    },
}
