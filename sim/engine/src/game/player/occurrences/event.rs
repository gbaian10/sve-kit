//! Completed event details without authority object, snapshot or event identities.

use crate::game::player::Viewer;
use crate::ir::{EvolutionState, Phase, Posture, ZoneKind};

/// Authorized facts distinguishing occurrences of the same ability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PendingEvent {
    /// Drive was acquired.
    DriveGained,
    /// A race completed.
    Race {
        /// Which race of the turn it was.
        ordinal: u64,
    },
    /// A card entered the field.
    EnterField {
        /// Rules names of the entering card.
        rules_names: Vec<String>,
        /// Zone it came from; absent when it was created.
        from: Option<ZoneKind>,
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
        value: u64,
    },
    /// Equipment was attached.
    Equip,
    /// The follower completed a normal limit break.
    LimitBreak,
    /// The follower completed a final limit break.
    FinalLimitBreak,
    /// Cards moved between zones.
    Move {
        /// Player whose zone they left.
        player: Viewer,
        /// Zone they left.
        from: ZoneKind,
        /// Zone they entered.
        to: ZoneKind,
    },
    /// A follower attacked.
    Attack {
        /// Which attack of the turn it was.
        ordinal: u64,
    },
    /// A phase began.
    Phase {
        /// The phase.
        phase: Phase,
        /// Turn number.
        turn: u64,
        /// Turn player.
        player: Viewer,
    },
    /// A follower evolved.
    Evolve {
        /// Evolved or super evolved.
        state: EvolutionState,
    },
    /// Leader life changed.
    LifeChange {
        /// Signed change.
        delta: i64,
        /// Whose leader.
        player: Viewer,
    },
    /// Damage was dealt.
    Damage {
        /// Amount dealt.
        amount: u64,
    },
}

/// Frozen public event context of one pending occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingEventContext {
    /// Completed event details, never parsed from the display description.
    pub event: PendingEvent,
    /// One-based source slot recorded at the event, when publicly described.
    pub position: Option<u64>,
}
