//! One seat's complete view of the board; the picture is redrawn from it alone.
//!
//! Unknown data is `null`, never a printed value standing in for the current one.
//! Hidden cards have no handle, placeholder or position here: only zone counts.

mod costs;
mod effects;
mod history;
mod processing;

pub use costs::{
    CardCostQuote, CostGroup, CostGroupRole, CostLine, CostObservation, CostSummary, PendingCost,
    PlayCostQuote, PpQuote, QuoteBasis,
};
pub use effects::{
    AbilityKind, AbilityQuota, ConsumptionPoint, CostKind, DamageKind, EffectExpiry,
    EffectInterval, EffectRecipient, GrantedDefinition, InstalledChange, InstalledEffect,
    NumberChange, PendingTrigger, PreventionChange, TriggerTiming,
};
pub use history::{
    CardKind, DamageHistory, DiceHistory, HistoryEvent, StatGainHistory, SubjectHistory,
    TurnHistory,
};
pub use processing::{
    Declared, DeclaredMode, DeclaredNumber, Procedure, ProcedureKind, ProcessingContext,
    ProcessingStage,
};

use crate::common::{CardRef, ObjectHandle, Seat, Subject};
use crate::number::{Int, Nat};
use alloc::collections::BTreeMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Whether both seats or only this viewer may know an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Knowledge {
    /// Known to both seats.
    Public,
    /// Known to this viewer but not to both seats.
    Private,
}

/// Which side of a card faces up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Facing {
    /// Face up.
    Up,
    /// Face down.
    Down,
}

/// Whether a card stands or has acted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Posture {
    /// Standing.
    Standing,
    /// Acted.
    Acted,
}

/// Evolution state of a field card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvolutionState {
    /// Not evolved.
    Base,
    /// Evolved.
    Evolved,
    /// Super evolved.
    SuperEvolved,
}

/// Rules phase of a turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Start phase.
    Start,
    /// Main phase.
    Main,
    /// End phase.
    End,
}

/// Effective characteristics, read from the authority and never recomputed from print.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Characteristics {
    /// Every current rules name; same-name checks use their intersection.
    pub rules_names: Vec<String>,
    /// Current card type, as the snapshot's rules term.
    pub card_type: String,
    /// Current traits.
    pub traits: Vec<String>,
    /// Current classes, when the authority provides them.
    pub classes: Option<Vec<String>>,
    /// Current cost, when the authority provides it.
    pub cost: Option<Int>,
}

/// Effective follower numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stats {
    /// Effective attack.
    pub attack: Int,
    /// Remaining effective life.
    pub life: Int,
}

/// A named counter on a card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Counter {
    /// Rules name or registered key of the counter.
    pub name: String,
    /// Current amount.
    pub amount: Nat,
}

/// Current state of an object whose state this viewer may know.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CardState {
    /// Current posture.
    pub posture: Posture,
    /// Current evolution state.
    pub evolution: EvolutionState,
    /// Effective numbers of a field follower; absent for anything else.
    pub stats: Option<Stats>,
    /// Effective keywords.
    pub keywords: Vec<String>,
    /// Current counters.
    pub counters: Vec<Counter>,
    /// Whether Box currently suppresses the card.
    pub boxed: bool,
    /// Authorized installed changes; the effective values above stay authoritative.
    pub effects: Vec<InstalledEffect>,
    /// Ability budgets: absent when unknown, empty when known to be unlimited.
    pub quotas: Option<Vec<AbilityQuota>>,
}

/// One currently identifiable object; each appears once in the object table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectView {
    /// Known printing and physical face.
    pub card: CardRef,
    /// Face supplying the current characteristics.
    pub effective_face: CardRef,
    /// Owner.
    pub owner: Seat,
    /// Current controller.
    pub controller: Seat,
    /// Who may know this object.
    pub knowledge: Knowledge,
    /// Side facing up.
    pub facing: Facing,
    /// Effective characteristics.
    pub characteristics: Characteristics,
    /// Current state; absent when only the face is authorized.
    pub state: Option<CardState>,
}

/// One known member of a zone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneMember {
    /// Authorized offset; absent for a slot the viewer may not locate.
    pub position: Option<Nat>,
    /// Key of the member in the object table.
    pub object: ObjectHandle,
}

/// Size of a zone and its authorized members.
///
/// The count minus the known members is the number of anonymous card backs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoneView {
    /// Full size, including anonymous members.
    pub count: Nat,
    /// Capacity from the rules, when the zone has one.
    pub capacity: Option<Nat>,
    /// Authorized members only.
    pub cards: Vec<ZoneMember>,
}

/// Every zone of one player; an absent zone is empty rather than missing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Zones {
    /// Main deck.
    pub deck: ZoneView,
    /// Evolve deck.
    pub evolve_deck: ZoneView,
    /// Hand.
    pub hand: ZoneView,
    /// Field.
    pub field: ZoneView,
    /// EX area.
    pub ex: ZoneView,
    /// Cemetery.
    pub cemetery: ZoneView,
    /// Banished cards.
    pub banish: ZoneView,
    /// Resolving cards.
    pub resolution: ZoneView,
    /// Evolve cards linked to field cards.
    pub evolution: ZoneView,
    /// Race area.
    pub race: ZoneView,
    /// Drive area.
    pub drive: ZoneView,
    /// Trigger area.
    pub trigger: ZoneView,
    /// Equipment area.
    pub equipment: ZoneView,
}

/// One player's public resources, effects and zones; the leader is not an object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayerView {
    /// Seat of this player.
    pub seat: Seat,
    /// Leader life.
    pub life: Int,
    /// Available play points.
    pub pp: Int,
    /// Maximum play points.
    pub max_pp: Int,
    /// Available evolution points.
    pub ep: Int,
    /// Available super evolution points.
    pub sep: Int,
    /// Disclosed installed effects on this player.
    pub effects: Vec<InstalledEffect>,
    /// Zone sizes and authorized members.
    pub zones: Zones,
}

/// Public turn state; there is no phase before the game starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TurnView {
    /// Turn player, once established.
    pub active: Option<Seat>,
    /// First player, once established.
    pub first_player: Option<Seat>,
    /// Current phase.
    pub phase: Option<Phase>,
    /// Completed turns in seat order.
    pub elapsed_turns: [Nat; 2],
}

/// A declared attack, so the arrow can be redrawn from the board alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttackView {
    /// Public attacking card.
    pub attacker: Subject,
    /// Public target: a leader or a card.
    pub target: Subject,
}

/// Kind of link between two objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    /// An evolve card on its base.
    Evolution,
    /// Equipment on its bearer.
    Equipment,
    /// A rider on its mount.
    Ride,
    /// A card in the race area and its field card.
    Race,
    /// A card in the drive area and its field card.
    Drive,
}

/// A typed link, listed only while this viewer can identify both ends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    /// Kind of link.
    pub kind: RelationshipKind,
    /// Attached object.
    pub from: ObjectHandle,
    /// Object it is attached to.
    pub to: ObjectHandle,
}

/// A face this viewer has seen, with no current location or copy count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RememberedCard {
    /// Remembered face.
    pub card: CardRef,
    /// Remembered owner.
    pub owner: Seat,
}

/// Complete authorized state for one seat at one stable stop.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BattleObservation {
    /// Seat this observation is authorized for.
    pub viewer: Seat,
    /// Every currently identifiable object, keyed by its handle.
    pub objects: BTreeMap<ObjectHandle, ObjectView>,
    /// Public turn state.
    pub turn: TurnView,
    /// Current attack, while one is declared.
    pub attack: Option<AttackView>,
    /// Both players in seat order.
    pub players: [PlayerView; 2],
    /// Typed links between identifiable objects.
    pub relationships: Vec<Relationship>,
    /// Authorized processing contexts from outermost to innermost.
    pub processing: Vec<ProcessingContext>,
    /// Cost quotes authorized for this seat.
    pub costs: CostObservation,
    /// Disclosed pending occurrences; none is answerable from here.
    pub pending: Vec<PendingTrigger>,
    /// Committed facts kept for this turn.
    pub history: TurnHistory,
    /// Faces seen earlier, without a link to any current object.
    pub remembered: Vec<RememberedCard>,
}
