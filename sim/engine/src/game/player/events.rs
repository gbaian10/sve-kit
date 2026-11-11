//! Committed, perspective-specific rules history. The current position remains `observe`.

mod journal;
mod ordering;
mod steps;

pub(in crate::game) use journal::{Journal, MovementBefore, PublishedEvents};

use super::observation::{CardProperties, EffectSource, ObservedZone, TurnObservation};
use super::{ObjectHandle, Viewer};
use crate::game::Game;
use crate::ir::{
    AbilityQualification, DamageFlags, EvolutionState, MoveMethod, PaymentTime, Stat, TriggerIcon,
};
use alloc::sync::Arc;
use core::fmt;

/// A cursor for one game and one authenticated perspective.
#[derive(Clone)]
pub struct EventId {
    stream: Arc<()>,
    index: u64,
}

impl PartialEq for EventId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.stream, &other.stream) && self.index == other.index
    }
}
impl Eq for EventId {}
impl fmt::Debug for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventId")
            .field("sequence", &self.index)
            .finish_non_exhaustive()
    }
}

/// A continuously identifiable card, or remembered characteristics without a current link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventCard {
    /// Absent when only a remembered face is authorized.
    pub handle: Option<ObjectHandle>,
    /// Characteristics frozen at the occurrence, using this viewer's permissions.
    pub properties: CardProperties,
}

/// A public endpoint. Void has no player zone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventLocation {
    /// Seat whose zone contains the card.
    pub player: Viewer,
    /// Absent for creation or erasure outside the game.
    pub zone: Option<ObservedZone>,
    /// Omitted for unauthorized identities and private unordered slots.
    pub position: Option<u64>,
    /// Authorized identity at this endpoint.
    pub card: Option<EventCard>,
}

/// Physical movement method, including rule and ordinary play procedures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementMethod {
    /// A typed instruction's original rules method.
    Effect(MoveMethod),
    /// A successful draw, including a rule draw.
    Draw,
    /// An actual discard.
    Discard,
    /// Ordinary movement whose procedure is given by its cause.
    Procedure,
}

/// Closed action vocabulary shared by the Step adapter and typed result adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    /// Proper card play.
    Play,
    /// Completion of a card's resolution.
    Resolve,
    /// Positive actual damage.
    Damage,
    /// A posture change to acted.
    Act,
    /// A posture change to standing.
    Stand,
    /// An authorized disclosure.
    Reveal,
    /// A visible facing change.
    Facing,
    /// Attack declaration.
    Attack,
    /// A destruction attempt or successful destruction trace.
    Destroy,
    /// A banish instruction.
    Banish,
    /// A replacement applied.
    Replacement,
    /// Removal from the game.
    Erase,
    /// Equipment association established.
    Equip,
    /// Evolution performed by the shared rule primitive.
    Evolve,
    /// A public counter change.
    Counter,
    /// A public resource change.
    Resource,
    /// Accepted die value, excluding intermediate reroll attempts.
    Dice,
    /// A performed shuffle, without a permutation.
    Shuffle,
    /// Fusion result.
    Fusion,
    /// Race result.
    Race,
    /// First Drive acquisition.
    DriveGained,
    /// Accepted Drive trigger.
    DriveTrigger,
    /// Completed Drive check.
    DriveChecked,
    /// Completed keyword payment, distinct from an ordinary counter removal.
    Payment,
    /// Effect-driven evolution, distinct from playing an evolution ability.
    EffectEvolve,
    /// Direct leader life change, independent of damage.
    Life,
    /// Explicit numeric facets.
    Stats,
    /// Proper ability play after payment.
    AbilityPlay,
    /// Completed ability resolution.
    AbilityResolve,
    /// A scheduled turn was skipped.
    TurnSkipped,
    /// A public controller change trace.
    Control,
}

/// Additional closed rule facets that cannot be inferred from the destination alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionDetails {
    /// Damage retains its combat/attack/ability classification.
    Damage(DamageFlags),
    /// Frozen public traits of the materials of one Fusion.
    Fusion(Vec<String>),
    /// The accepted trigger icon.
    DriveTrigger(TriggerIcon),
    /// A completed check and its actual destination, without an object link.
    DriveChecked {
        /// Whether its trigger was accepted.
        accepted: bool,
        /// Owner of the final zone.
        player: Viewer,
        /// Actual final zone.
        zone: ObservedZone,
    },
    /// Successful keyword payment facets, independent of receipt identities.
    Payment {
        /// Rules point of the payment.
        time: PaymentTime,
        /// Whether the payment completed.
        complete: bool,
        /// Whether it was a zero payment.
        zero: bool,
        /// Whether replacement affected it.
        replaced: bool,
    },
    /// Evolution result, retaining effect-driven evolution's role.
    Evolution(EvolutionState),
    /// Proper-play qualification, including Fusion and Union Burst.
    Ability(AbilityQualification),
    /// A publicly skipped normal or additional turn.
    TurnSkipped {
        /// Whether this was an additional turn.
        extra: bool,
    },
}

/// An authorized participant; hidden cards have no placeholder identity.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    variant_size_differences,
    reason = "Card data is boxed while leader identities remain inline."
)]
pub enum EventSubject {
    /// Public leader identity.
    Leader(Viewer),
    /// Authorized card characteristics and optional current identity.
    Card(Box<EventCard>),
}

/// One explicit numeric facet; it contains no receipt, instruction or effect identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberFacet {
    /// Numeric characteristic, when this is a stat change.
    pub stat: Option<Stat>,
    /// Previous value, when provided by the rule result.
    pub before: Option<i64>,
    /// Resulting value, when provided by the rule result.
    pub after: Option<i64>,
    /// Actual amount or delta.
    pub amount: i64,
}

/// A closed action result. Missing participants are unauthorized, not authority IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionEvent {
    /// Rules meaning of this occurrence.
    pub kind: ActionKind,
    /// Seat in this kind's rules role: performer, damage controller, payer, roller,
    /// life subject or deck owner, when the source provides one.
    pub player: Option<Viewer>,
    /// Authorized participants in source-defined roles.
    pub subjects: Vec<Option<EventSubject>>,
    /// Authorized historical source, when supplied by a typed execution.
    pub source: Option<EffectSource>,
    /// Explicit numeric results.
    pub numbers: Vec<NumberFacet>,
    /// Rules name of a resource or counter; never an ability reference.
    pub name: Option<String>,
    /// Facing result when applicable.
    pub face_up: Option<bool>,
    /// Additional rules distinctions from the typed or shared producer.
    pub details: Option<ActionDetails>,
}

/// A committed rules occurrence, without the authority's open JSON event packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// One physical move. A draw or discard enriches this same occurrence.
    Movement {
        /// Method retained independently of its destination.
        method: MovementMethod,
        /// Authorized endpoint immediately before mutation.
        before: Box<EventLocation>,
        /// Authorized endpoint after knowledge changes and before follow-up rules.
        after: Box<EventLocation>,
    },
    /// A rule or typed result without a separate physical movement.
    Action(ActionEvent),
    /// Entry into a rules phase, before its associated automatic actions.
    Phase(TurnObservation),
    /// A rules win or loss, including concession and simultaneous defeat.
    Result {
        /// Player declared winner by this source, if applicable.
        winner: Option<Viewer>,
        /// Player declared loser by this source, if applicable.
        loser: Option<Viewer>,
        /// Public rules clause when supplied by the source.
        rule: Option<String>,
    },
}

/// A viewer-local occurrence. Hidden sources do not consume any public coordinate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerEvent {
    /// Cursor for the next read.
    pub id: EventId,
    /// Viewer-local cause coordinate.
    pub cause: u64,
    /// Viewer-local simultaneous group coordinate.
    pub group: u64,
    /// An already public causal parent, if one exists.
    pub parent: Option<EventId>,
    /// Frozen authorized rules result.
    pub kind: EventKind,
}

/// Event history is unavailable; this does not stop input or observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventError {
    /// This cursor belongs to another game/viewer or a future occurrence.
    InvalidCursor,
    /// A source cannot be projected completely and safely for this viewer.
    ProjectionGap,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Events share the game authority without changing its readers."
)]
impl Game {
    /// Read committed occurrences after this viewer's cursor without advancing rules.
    ///
    /// # Errors
    /// Rejects foreign/future cursors and a fixed perspective-local projection gap.
    pub fn events(
        &self,
        viewer: Viewer,
        after: Option<&EventId>,
    ) -> Result<impl Iterator<Item = &PlayerEvent>, EventError> {
        self.player_events.read(viewer, after)
    }
}
