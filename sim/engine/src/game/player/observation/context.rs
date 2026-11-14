//! Public rule state and current-turn history without authority references.

use super::{EffectSource, ObservedZone};
use crate::game::player::{ObjectHandle, Viewer};
use crate::ir::{AbilityKind, CardKind, ConsumptionPoint};

/// A currently identifiable effect recipient.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EffectRecipient {
    /// A leader.
    Leader(Viewer),
    /// A currently authorized card.
    Card(ObjectHandle),
}

/// Damage replacement operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreventionChange {
    /// Prevent the covered damage.
    Prevent,
    /// Add a signed amount to the covered damage.
    Add(i64),
    /// Each covered damage of at least `threshold` becomes `replacement`.
    Cap {
        /// Smallest single damage that is replaced.
        threshold: u64,
        /// Damage dealt instead.
        replacement: u64,
    },
}

/// Trigger event category without a payload, binding or event identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerTiming {
    /// A card enters the field.
    EnterField,
    /// Targets are declared.
    Targeted,
    /// An ability resolves.
    Resolve,
    /// A payment commits.
    Payment,
    /// Counters change.
    CounterChange,
    /// A card enters EX.
    EnterEx,
    /// A card leaves the field.
    LeaveField,
    /// A follower attacks.
    Attack,
    /// A follower evolves.
    Evolve,
    /// A phase begins.
    Phase,
    /// A stat changes.
    StatChange,
    /// Posture changes.
    Posture,
    /// A card is destroyed.
    Destroy,
    /// Union Burst is activated.
    UnionBurst,
    /// Cards are fused.
    Fusion,
    /// A card is played.
    Play,
    /// Cards are discarded.
    Discard,
    /// Cards are banished.
    Banish,
    /// Cards are drawn.
    Draw,
    /// Drive is acquired.
    DriveGained,
    /// A Drive check completes.
    DriveChecked,
    /// A Drive trigger completes.
    DriveTrigger,
    /// A die roll completes.
    Dice,
    /// Equipment is attached.
    Equip,
    /// A card moves.
    Move,
    /// A race completes.
    Race,
    /// Leader life changes.
    LifeChange,
    /// Damage is dealt.
    Damage,
}

/// One disclosed pending occurrence; never an answerable option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingTrigger {
    /// Printed text and separately authorized current link.
    pub source: EffectSource,
    /// Controller of the occurrence.
    pub controller: Viewer,
    /// Completed triggering event kind.
    pub timing: TriggerTiming,
    /// Remaining selections in an admission group, only for its controller.
    pub admission_remaining: Option<u64>,
}

/// Current ability budget, without an allocated instance identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbilityQuota {
    /// Authorized text origin of this definition.
    pub source: EffectSource,
    /// Ability category.
    pub kind: AbilityKind,
    /// Limit per turn.
    pub maximum: u64,
    /// Remaining uses under the authoritative budget.
    pub remaining: u64,
    /// Rules stage that consumes the use.
    pub consume_at: ConsumptionPoint,
}

/// Public subject fact category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryEvent {
    /// Formal card play committed.
    Play,
    /// Attack committed.
    Attack,
    /// Evolution committed.
    Evolve,
    /// Departure committed.
    Leave,
}

/// A committed public subject fact from the current turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubjectHistory {
    /// Completed event kind.
    pub event: HistoryEvent,
    /// Current object only during the captured identification period and public generation.
    pub current: Option<ObjectHandle>,
    /// Controller at the event.
    pub controller: Viewer,
    /// Active player at the event.
    pub active: Viewer,
    /// Public card kind at the event.
    pub kind: CardKind,
    /// Recorded class, when needed by the authoritative fact.
    pub class: Option<String>,
    /// Recorded traits at the event.
    pub traits: Vec<String>,
    /// Recorded names, absent if the fact did not capture them.
    pub names: Option<Vec<String>>,
    /// Whether the subject was a token.
    pub token: bool,
    /// Whether the departure was destruction.
    pub destroyed: bool,
    /// Departure destination, when present.
    pub destination: Option<ObservedZone>,
}

/// Committed damage links retained by the current-turn authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageHistory {
    /// Current damaged card, when still authorized.
    pub recipient: Option<ObjectHandle>,
    /// Current source card, when still authorized.
    pub source: Option<ObjectHandle>,
    /// Source controller at the event.
    pub controller: Viewer,
}

/// Positive facets of one committed stat change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatGainHistory {
    /// Current subject, when still authorized.
    pub current: Option<ObjectHandle>,
    /// Attack increased.
    pub attack: bool,
    /// Maximum life increased.
    pub life: bool,
}

/// Final accepted public die value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiceHistory {
    /// Rolling player.
    pub player: Viewer,
    /// Accepted value after rerolls.
    pub value: u64,
}

/// Current-turn committed information, separate from persistent catalog memory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TurnHistory {
    /// Public card and follower events.
    pub subjects: Vec<SubjectHistory>,
    /// Public damage links used by current-turn queries.
    pub damage: Vec<DamageHistory>,
    /// Positive stat-change facets.
    pub stat_gains: Vec<StatGainHistory>,
    /// Accepted dice values.
    pub dice: Vec<DiceHistory>,
    /// Committed card plays in seat order.
    pub played: [u64; 2],
    /// Committed Union Burst uses in seat order.
    pub union_bursts: [u64; 2],
}
