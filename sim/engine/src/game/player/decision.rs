//! Player questions describe the operation without exposing authority continuations.

use super::observation::ObservedZone;
use super::observation::{CardFace, CostSummary};
use super::{AbilityChoice, PendingChoice, QuickChoice, QuotaQuestion, ReplacementChoice};
use super::{
    DistributionQuestion, ObjectHandle, SelectionDestination, SelectionGroup, SelectionQuestion,
    Viewer,
};
use crate::ir::{Cardinality, ModeId, Nat, Position, QuickWindow, TriggerIcon};
use alloc::collections::BTreeMap;

/// One affordable exchange route with its conditional pre-payment quotation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExchangeRouteChoice {
    /// Original route coordinate, including gaps.
    pub index: Nat,
    /// This route is fixed for the quote; its materials remain unselected.
    pub cost: CostSummary,
}

/// Destinations proved by the native previews before asking for a route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchReplacementQuestion {
    /// Absent when one destination cannot describe every found card.
    pub original: Option<SelectionDestination>,
    /// Absent when one destination cannot describe every found card.
    pub replacement: Option<SelectionDestination>,
}

/// One printed choice addressed only within the current question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardOption {
    /// Native answer coordinate, valid only with this question's token.
    pub handle: u64,
    /// Authorized catalogue printing, shared by identical copies.
    pub card: String,
    /// Authorized rules name supplied by the original question.
    pub name: String,
    /// Current Observation capability, never a physical identity.
    pub object: Option<ObjectHandle>,
    /// Complete frozen names when distinct-name selection requires them.
    pub rules_names: Option<Vec<String>>,
    /// Original cost when a total or distinct-cost constraint requires it.
    pub original_cost: Option<i64>,
}

/// Direction supplied by the native ordering procedure, never inferred by a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderDirection {
    /// The first item is above every later item within the ordered segment.
    TopDown,
}

/// Placement stays tied to the same frozen native permutation as its candidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderQuestion {
    /// Every frozen candidate must appear exactly once in the answer.
    pub cards: Vec<CardOption>,
    /// Native interpretation of the answer's sequence.
    pub direction: OrderDirection,
    /// The procedure's receiving player and zone, independent of projection context.
    pub destination: SelectionDestination,
    /// The procedure's receiving edge.
    pub position: Position,
}

/// Search bounds and printed groups, retaining the original answer coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchQuestion {
    /// Only the searching actor receives these printed choices.
    pub cards: Vec<CardOption>,
    /// Frozen authorized provenance; absent when the producer did not capture it.
    pub facts: Option<SearchFacts>,
    /// Effective total bounds.
    pub cardinality: Cardinality,
    /// Every printed group, including empty entries.
    pub groups: Option<Vec<SelectionGroup>>,
    /// No rules name may occur in two chosen cards.
    pub distinct_names: bool,
    /// Upper bound across every selected group together.
    pub maximum_original_cost: Option<i64>,
}

/// Search identifies printings without granting knowledge of their hidden deck slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchFacts {
    /// Receiving the question does not change the candidates' actual source player.
    pub origin: SelectionDestination,
    /// Exact physical faces, indexed by this question's native answer coordinates.
    pub faces: BTreeMap<u64, CardFace>,
}

/// A selection from the frozen remaining search or movement candidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartitionQuestion {
    /// Question-local candidates without authority object identities.
    pub cards: Vec<CardOption>,
    /// Exact or bounded number accepted by the native procedure.
    pub cardinality: Cardinality,
}

/// One search allocation stop, distinct from a generic movement capacity selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchPartitionQuestion {
    /// Complete remaining set and effective bounds for this stop.
    pub selection: PartitionQuestion,
    /// Absent when the producer did not capture the placement contract.
    pub facts: Option<SearchPartitionFacts>,
}

/// Placement and candidate facts captured together with the native continuation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchPartitionFacts {
    /// Authorized faces and source of the complete remaining set.
    pub candidates: SearchFacts,
    /// Receiving player and zone of this one allocation.
    pub destination: SelectionDestination,
    /// Known edge, or no edge for the admitted non-deck destinations.
    pub position: Option<Position>,
    /// Whether the native procedure commits the answer's item sequence.
    pub ordered: bool,
    /// None means unselected cards stay in their source at this stop.
    pub remainder_destination: Option<SelectionDestination>,
}

/// The existing answer channel for one specific discard operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscardKind {
    /// Resolution discard, answered with `Answer::Targets`.
    Effect,
    /// Declared hand payment, answered with `Answer::CostMaterials`.
    Cost,
}

/// Native procedure that prepared an integer question, independent of its bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerSource {
    /// The X payment parameter prepared by the play procedure.
    XCost,
    /// A numeric declaration instruction during effect resolution.
    NumberDeclaration,
    /// Another native integer decision without a supported wire use.
    Other,
}

/// Capacity of a movement batch, answered with `Answer::SearchPartition`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovementCapacityQuestion {
    /// Player receiving this batch.
    pub destination: Viewer,
    /// Field or EX area whose remaining spaces constrain the answer.
    pub zone: ObservedZone,
    /// Exact number of candidates that must be selected.
    pub available: Nat,
    /// Existing cards and prospective token printings, before creation or entry.
    pub selection: PartitionQuestion,
    /// Frozen identities; absent when the producer did not capture its candidate facts.
    pub facts: Option<BTreeMap<u64, CapacityCard>>,
}

/// Existing cards and prospective tokens must never share an invented current capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapacityCard {
    /// A physical card whose printing is authorized by the frozen native domain.
    Existing {
        /// Physical face, independent of its effective characteristics.
        face: CardFace,
        /// Controller only while the candidate was already identifiable to the chooser.
        controller: Option<Viewer>,
    },
    /// A printing selected by the native creation procedure, before an object exists.
    Token {
        /// Face the shared native creation procedure will actually create.
        face: CardFace,
    },
}

/// Token creation capacity, answered with `Answer::TokenCapacity`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenCapacityQuestion {
    /// Player receiving the created tokens.
    pub destination: Viewer,
    /// Field or EX destination.
    pub zone: ObservedZone,
    /// Available spaces when this question was prepared.
    pub available: Nat,
    /// Original answer indices and their complete legal token-name combinations.
    pub choices: Vec<Vec<String>>,
    /// Exact creation faces for each original combination, when provided by its frozen native wait.
    pub faces: Option<Vec<Vec<CardFace>>>,
    /// Per-token destinations for an allocation across one or more zones.
    pub placements: Option<Vec<Vec<ObservedZone>>>,
}

/// Closed, actor-authorized decision shapes, separate from the native execution domain.
///
/// Existing target and occurrence payloads contain only question-local coordinates.
/// Native validation still owns admissibility, payment and generation checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionShape {
    /// Public roll and unused permissions; answer `Integer(0|1)`.
    DiceReroll {
        /// Current public die result.
        value: Nat,
        /// Remaining rerolls.
        remaining: Nat,
    },
    /// Effective definitions; answer `Ability(handle)`.
    Abilities(Vec<AbilityChoice>),
    /// Evolution-equivalent resources; answer `ResourceEntryPayment`.
    ResourceEntryPayment {
        /// Question-local private resource choices.
        materials: SelectionQuestion,
        /// Whether one EP can replace one PP.
        ep_allowed: bool,
    },
    /// Optional entry posture; answer `WardEntry`.
    WardEntry {
        /// Printing known to its entering controller.
        card: String,
        /// Frozen physical source face, including a not-yet-entered token.
        face: CardFace,
        /// Authorized printed name.
        name: String,
        /// Current source zone.
        zone: String,
        /// One-based source position when the actor can locate the arriving card.
        position: Option<u64>,
        /// Current Observation capability of the entering card, when still identifiable.
        object: Option<ObjectHandle>,
    },
    /// Rule-owned Quick plays and explicit Pass; answer `Quick`.
    Quick {
        /// Original rule window.
        window: QuickWindow,
        /// Private plays, visible only to their actor.
        options: Vec<QuickChoice>,
    },
    /// Accept or decline a declaration payment; answer `OptionalCost`.
    OptionalCost,
    /// Accept or decline a resolution payment; answer `OptionalCost`.
    ResolutionPayment,
    /// Head-cost materials; answer `CostMaterials`.
    CostMaterials(SelectionQuestion),
    /// A hand discard with its original bounds and answer channel.
    Discard {
        /// Resolution effect or play cost.
        kind: DiscardKind,
        /// Authorized hand choices and exact count.
        cards: SelectionQuestion,
    },
    /// Genuine affordable route indices, without filling gaps; answer `Integer(route)`.
    ExchangeRoutes(Vec<ExchangeRouteChoice>),
    /// Optional resolution body; answer `Integer(0)` to decline or `Integer(1)` to accept.
    May,
    /// Optional draw continuation; answer `Stepwise`.
    BeforeDraw,
    /// Pending occurrences; answer `Pending(handle)`.
    Pending(Vec<PendingChoice>),
    /// Simultaneous limited admission; answer `TriggerQuota`.
    TriggerQuota(QuotaQuestion),
    /// A printed numeric parameter; answer `Integer` within these bounds.
    Integer {
        /// Original effective bounds.
        bounds: Cardinality,
        /// Origin recorded by the native Wait producer, never inferred from bounds.
        source: IntegerSource,
    },
    /// Frozen permutation; answer `Order` with every handle once.
    Order(OrderQuestion),
    /// Replacement order; answer `Order`.
    DamageReplacements(Vec<ReplacementChoice>),
    /// Public icon and optional target; answer `DriveTrigger`.
    DriveTrigger {
        /// Already revealed trigger icon.
        icon: TriggerIcon,
        /// Controller field candidates, without physical identities.
        targets: Vec<CardOption>,
    },
    /// One complete search answer, including all groups; answer `Search`.
    Search(SearchQuestion),
    /// Remaining found cards for one printed destination; answer `SearchPartition`.
    SearchPartition(SearchPartitionQuestion),
    /// Arrivals competing for field or EX space; answer `SearchPartition`.
    MovementCapacity(MovementCapacityQuestion),
    /// Frozen recipients and totals; answer `Distribution`.
    Distribution(DistributionQuestion),
    /// Hand-movement replacement; answer `SearchReplacement`.
    SearchReplacement(SearchReplacementQuestion),
    /// Declared or resolution targets; answer `Targets` or `TargetsWithRoles`.
    Targets(SelectionQuestion),
    /// Token-name combinations constrained by destination capacity.
    TokenCapacity(TokenCapacityQuestion),
    /// Declaration modes; answer `Modes`.
    Modes {
        /// Native printed mode identifiers, in resolution order.
        modes: Vec<ModeId>,
        /// Original count bounds.
        cardinality: Cardinality,
        /// Evaluated maximum number of selections.
        count: Nat,
    },
    /// Exactly one resolution alternative; answer `Modes`.
    ResolutionAlternative {
        /// Printed alternative identifiers, never reordered or renumbered.
        modes: Vec<ModeId>,
        /// Original count bounds.
        cardinality: Cardinality,
        /// Evaluated number of alternatives to choose.
        count: Nat,
    },
}
