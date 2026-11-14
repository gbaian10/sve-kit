//! Committed public facts of the current turn; not a replay and not provisional.

use crate::common::{ObjectHandle, Seat, Zone};
use crate::number::Nat;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Category of a committed subject fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HistoryEvent {
    /// A card play committed.
    Play,
    /// An attack committed.
    Attack,
    /// An evolution committed.
    Evolve,
    /// A departure committed.
    Leave,
}

/// Card kind recorded with a subject fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CardKind {
    /// Crest.
    Crest,
    /// Evolved follower.
    EvolveFollower,
    /// Follower.
    Follower,
    /// Spell.
    Spell,
    /// Amulet.
    Amulet,
}

/// One committed public fact about a card this turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubjectHistory {
    /// Category of the fact.
    pub event: HistoryEvent,
    /// Current object, only within the identification period captured by the fact.
    pub current: Option<ObjectHandle>,
    /// Controller at the event.
    pub controller: Seat,
    /// Turn player at the event.
    pub active: Seat,
    /// Public card kind at the event.
    pub card_kind: CardKind,
    /// Recorded class, when the fact needed it.
    pub class: Option<String>,
    /// Recorded traits.
    pub traits: Vec<String>,
    /// Recorded rules names; absent when the fact did not capture them.
    pub names: Option<Vec<String>>,
    /// Whether the subject was a token.
    pub token: bool,
    /// Whether the departure was a destruction.
    pub destroyed: bool,
    /// Destination of the departure, when there was one.
    pub destination: Option<Zone>,
}

/// Committed damage links kept for queries about this turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DamageHistory {
    /// Damaged card, while still authorized.
    pub recipient: Option<ObjectHandle>,
    /// Source card, while still authorized.
    pub source: Option<ObjectHandle>,
    /// Controller of the source at the event.
    pub controller: Seat,
}

/// Which numbers one committed stat change increased.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatGainHistory {
    /// Subject, while still authorized.
    pub current: Option<ObjectHandle>,
    /// Whether attack increased.
    pub attack: bool,
    /// Whether maximum life increased.
    pub life: bool,
}

/// A final accepted public die value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiceHistory {
    /// Rolling player.
    pub player: Seat,
    /// Accepted value after rerolls.
    pub value: Nat,
}

/// Committed information of the current turn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TurnHistory {
    /// Public card and follower facts.
    pub subjects: Vec<SubjectHistory>,
    /// Public damage links.
    pub damage: Vec<DamageHistory>,
    /// Stat increases.
    pub stat_gains: Vec<StatGainHistory>,
    /// Accepted dice values.
    pub dice: Vec<DiceHistory>,
    /// Committed card plays in seat order.
    pub played: [Nat; 2],
    /// Committed Union Burst uses in seat order.
    pub union_bursts: [Nat; 2],
}
