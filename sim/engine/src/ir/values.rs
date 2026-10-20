use super::{
    AbilityPredicate, AbilityReference, AttackEvent, BanishEvent, Characteristics,
    CounterChangeEvent, DamageEvent, DamageRecipient, DestroyEvent, DiceEvent, DiscardEvent,
    DrawEvent, DriveCheckedEvent, DriveGainedEvent, DriveTriggerEvent, EarthRitePaidComponent,
    EnterExEvent, EnterFieldEvent, EquipEvent, Event, EventSnapshotPolicy, EvolutionStateFields,
    EvolveEvent, FusedEvent, HistoryCount, HistoryQuery, Info, LeaveFieldEvent, LifeChangeEvent,
    MeasuredReceipt, MeasuredReceiptList, MoveEvent, MoveResult, MovementReceipt,
    MovementReceiptList, NonEmpty, NumericHistoryQuery, ObjectReceipt, ObjectReceiptList,
    ObjectRef, PaymentEvent, PaymentReceipt, PaymentResult, PhaseEvent, PhaseFields, PlayEvent,
    PlaySubject, PostureEvent, PostureFields, RaceEvent, ReceiptList, ReceiptReference,
    ReceiptStatus, ResolveEvent, RuntimeAbilityReference, RuntimeObjectView, SourcePredicate,
    StatChangeEvent, StatDelta, TargetedEvent, TokenOutcome, TurnIdFields, UbActivatedEvent,
    ValueEquality, Visitor, Walk,
};
use core::fmt::Debug;
use core::marker::PhantomData;
use core::num::NonZeroU64;

macro_rules! identifiers {
    ($($name:ident),* $(,)?) => { $(
        #[doc = concat!("Distinct internal ", stringify!($name), ".")]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name(pub u64);
        impl Walk for $name {
            fn walk(&self, _visitor: &mut Visitor, _path: &str) {}
        }
    )* };
}
identifiers!(
    ObjectId,
    Generation,
    ReceiptId,
    InstructionId,
    ScopeId,
    EventId,
    SnapshotId,
    ExecutionId,
    DecisionId,
    ResultMemberId,
    AbilityId,
    AbilityInstanceId,
    EffectId,
    EffectDefinitionId,
    AssociationId,
    BatchId,
    TurnId,
    Timestamp,
    AnchorId,
    RollId,
    CheckId,
    RuleId,
    RngRequest,
    ModeId,
    ScheduleToken,
    CommitId,
    OriginFactId,
    UsageId
);

/// Signed arithmetic domain; never implicitly coerced to a quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Int(pub i64);
/// Nonnegative domain, distinct from signed arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nat(pub u64);
/// A boolean domain, distinct from numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bool(pub bool);

/// A binding whose value type cannot change between definition and use.
#[derive(Debug, PartialEq, Eq)]
pub struct Binding<T> {
    id: u64,
    scope: ScopeId,
    marker: PhantomData<T>,
}
impl<T> Copy for Binding<T> {}
impl<T> Clone for Binding<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Binding<T> {
    pub(super) const fn key(self) -> (ScopeId, u64) {
        (self.scope, self.id)
    }

    /// Construct an internal scoped reference; lowering validates dominance.
    #[must_use]
    pub const fn new(id: u64, scope: ScopeId) -> Self {
        Self {
            id,
            scope,
            marker: PhantomData,
        }
    }
}
impl<T> Walk for Binding<T> {
    fn walk(&self, _visitor: &mut Visitor, _path: &str) {}
}
/// A frozen value reference is not a Live read or an object identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureRef<T> {
    /// Scoped frozen value.
    pub binding: Binding<T>,
}
impl<T> Walk for CaptureRef<T> {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        visitor.require("value.capture", path);
    }
}

pub(super) mod sealed;
/// Closed result domain of a pure expression.
pub trait Value: sealed::Value + Clone + Debug + Eq {
    /// Reads legal for this result domain.
    type Read: Clone + Debug + Eq;
    /// Admissible static literals; runtime identity domains use `NoRead`.
    type Literal: Clone + Debug + Eq;
    /// Pure operations legal for this result domain.
    type Operation: Clone + Debug + Eq;
    /// Frozen event or receipt projections for this exact result domain.
    type Frozen: Clone + Debug + Eq;
}
/// A typed, pure value expression; capture and Live remain distinct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr<T: Value> {
    /// Literal in the exact value domain.
    Literal(T::Literal),
    /// Typed event and receipt projections never reread live state.
    Frozen(Box<T::Frozen>),
    /// Evaluated in the view at the consuming instruction's start.
    Live(Box<T::Read>),
    /// A previously frozen value.
    Captured(CaptureRef<T>),
    /// A scoped, already assigned value.
    Bound(Binding<T>),
    /// Pure operation with typed operands.
    Compute(Box<T::Operation>),
}
impl<T: Value> Walk for Expr<T>
where
    T::Literal: Walk,
    T::Read: Walk,
    T::Operation: Walk,
    T::Frozen: Walk,
{
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        match self {
            Self::Literal(value) => {
                visitor.require("shape.Expr.Literal", path);
                value.walk(visitor, path);
            }
            Self::Frozen(read) => {
                visitor.require("shape.Expr.Frozen", path);
                read.walk(visitor, &format!("{path}/frozen"));
            }
            Self::Live(read) => {
                visitor.require("shape.Expr.Live", path);
                visitor.require("value.live", path);
                read.walk(visitor, &format!("{path}/live"));
            }
            Self::Captured(reference) => {
                visitor.require("shape.Expr.Captured", path);
                reference.walk(visitor, path);
            }
            Self::Bound(binding) => {
                visitor.require("shape.Expr.Bound", path);
                binding.walk(visitor, path);
            }
            Self::Compute(operation) => {
                visitor.require("shape.Expr.Compute", path);
                operation.walk(visitor, &format!("{path}/compute"));
            }
        }
    }
}
macro_rules! domain {
    ($value:ty, $read:ty, $operation:ty, $frozen:ty, $literal:ty) => {
        impl sealed::Value for $value {}
        impl Value for $value {
            type Read = $read;
            type Literal = $literal;
            type Operation = $operation;
            type Frozen = $frozen;
        }
    };
}
domain!(Int, IntRead, IntOperation, IntFrozen, Int);
domain!(Nat, NatRead, NatOperation, NatFrozen, Nat);
domain!(Bool, BoolRead, Predicate, BoolFrozen, Bool);
domain!(CardName, NoRead, NameOperation, NoRead, CardName);
domain!(
    Info<CollaborationName>,
    NameRead,
    NoRead,
    NoRead,
    Info<CollaborationName>
);
domain!(ZoneRef, ZoneRead, ZoneOperation, ZoneFrozen, ZoneRef);
impl Walk for Int {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        if self.0 < 0 {
            visitor.require("value.signed-literal", path);
        }
    }
}
impl Walk for Nat {
    fn walk(&self, _visitor: &mut Visitor, _path: &str) {}
}
impl Walk for Bool {
    fn walk(&self, _visitor: &mut Visitor, _path: &str) {}
}

macro_rules! names {
    ($($name:ident),* $(,)?) => { $(
        #[doc = concat!("Registry-backed ", stringify!($name), "; not an operation name.")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name(pub String);
        impl Walk for $name { fn walk(&self, _visitor: &mut Visitor, _path: &str) {} }
    )* };
}
names!(
    CollaborationName,
    CardName,
    TraitId,
    TokenTemplateId,
    ClassId,
    TitleId,
    FaceId,
    CounterKind
);
tags!(CardKind {
    Follower,
    Spell,
    Amulet
});
tags!(ZoneKind {
    Deck,
    Hand,
    Field,
    Ex,
    Cemetery,
    Banish,
    Evolve,
    Resolution,
    Race,
    Drive,
    Trigger,
    Equipment
});
tags!(CharacteristicView {
    Original,
    Effective,
    LastKnown,
    ProspectiveEntry
});
tags!(Posture { Standing, Acted });
tags!(EvolutionState {
    Base,
    Evolved,
    SuperEvolved
});
tags!(Stat { Attack, Life });
tags!(Phase { Start, Main, End });
tags!(Edge { Begin, End });
tags!(Role { Owner, Controller });
tags!(Comparison {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual
});
tags!(Arithmetic {
    Add,
    Subtract,
    Multiply,
    Minimum,
    Maximum,
    FloorDivide,
    HalfUp
});
tags!(PlayerRelation {
    SelfPlayer,
    Opponents,
    All,
    Bound
});
tags!(TextQuantifier { Implicit, All, One });
tags!(RuleCarry {
    ExToResolutionEffect,
    ExToResolutionCounter,
    ResolutionToFieldEffect,
    ResolutionToFieldCounter
});

record!(ZoneRef {
    player: PlayerRef,
    kind: ZoneKind
});
/// Bounded and unbounded selection require different semantic support.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cardinality {
    /// Minimum successful choices.
    pub minimum: Nat,
    /// None is unbounded, distinct from a finite maximum.
    pub maximum: Option<Nat>,
}
impl Walk for Cardinality {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        visitor.require(
            if self.maximum.is_some() {
                "cardinality.bounded"
            } else {
                "cardinality.unbounded"
            },
            path,
        );
    }
}
record!(PlayerSet {
    relation: PlayerRelation,
    relative_to: PlayerRef,
    cardinality: Cardinality,
    text_quantifier: TextQuantifier
});
variants!(PlayerRef {
    FromExpression { value: Box<Expr<PlayerRef>> } => [],
    Controller {} => [], Owner {} => [], Active {} => [],
    Bound { binding: Binding<PlayerRef> } => [],
    RequireOne { players: Box<PlayerSet> } => ["player.require-one"]
});
variants!(ObjectView {
    Snapshot { value: Box<Expr<RuntimeObjectView>> } => ["reference.snapshot"],
    Current { object: Box<ObjectOperand> } => ["reference.object-generation"],
    LastKnown { object: Box<ObjectOperand>, reason: LkiReason } => ["reference.lki"],
    EventSnapshot { object: Box<ObjectOperand>, event: EventContext, policy: EventSnapshotPolicy } => ["value.event-snapshot"]
});
tags!(LkiReason {
    ZoneExit,
    SourceDeparted,
    CostMaterial
});
variants!(#[derive(Copy)] FollowPermission {
    SameEffectExplicitReference {} => [],
    TriggerExplicitReference { event: EventContext } => [],
    RuleCarry { rule: RuleCarry } => []
});
record!(
    MovedRef {
        receipt: MovementReceipt,
        member: Binding<ResultMemberId>,
        permission: FollowPermission
    }
);
variants!(ObjectOperand {
    // This follows only the triggering subject's before member through its header receipt.
    // Lowering must prove explicit card-text permission; payload after fields alone grant none.
    FollowEventMove { event: MovementEvent } => ["reference.trigger-follow", "reference.follow-generation", "read.event-header.move-receipt"],
    FromExpression { value: Box<Expr<ObjectRef>> } => ["reference.expression"],
    SourceHolder {} => ["reference.object-generation"],
    Target { binding: Binding<ObjectRef>, selection: CaptureRef<Selector>, requirements: Box<Expr<Bool>> } => ["reference.target"],
    Bound { binding: Binding<ObjectRef> } => [],
    Moved { reference: MovedRef } => ["reference.follow-generation", "receipt.move-after"]
});
variants!(Selector {
    CollectedMembers { receipts: ObjectReceiptList, projection: MemberProjection, deduplicate: GenerationDeduplication } => ["read.receipt.collected-members"],
    MoveAfterAll { receipts: MovementReceiptList, permission: FollowPermission, deduplicate: GenerationDeduplication } => ["receipt.move-after-all", "reference.follow-generation"],
    Created { receipt: ReceiptReference<TokenOutcome> } => ["receipt.created"],
    ReceiptMembers { receipt: ObjectReceipt, projection: MemberProjection } => ["read.receipt.members"],
    MoveAfter { receipt: MovementReceipt, permission: FollowPermission } => ["reference.follow-generation", "receipt.move-after"],
    Zone { players: PlayerSet, zone: ZoneKind, view: SelectionView, binding: Binding<ObjectRef>, predicate: Box<Expr<Bool>> } => ["selector.zone"],
    Filter { input: Box<Selector>, binding: Binding<ObjectRef>, predicate: Box<Expr<Bool>> } => ["selector.filter"],
    Union { sets: Vec<Selector> } => ["selector.union"],
    Intersection { sets: Vec<Selector> } => ["selector.intersection"],
    Difference { input: Box<Selector>, except: Box<Selector> } => ["selector.difference"],
    Ordered { zone: ZoneRef, start: Position, count: Expr<Nat>, direction: SliceDirection } => ["selector.ordered", "value.live"],
    Objects { objects: Vec<ObjectOperand> } => ["selector.objects"],
    Captured { reference: CaptureRef<Selector> } => [],
    Bound { reference: Binding<Selector> } => [],
    Relation { object: ObjectView, relation: ObjectRelation } => ["selector.relation"]
});
tags!(ObjectRelation {
    Equipment,
    Evolution,
    Race,
    Drive,
    AbilityHolder,
    Provider
});
variants!(#[derive(Copy)] SelectionView { Current {} => ["value.live"], EventSnapshot { event: EventContext } => [], Historical { snapshot: Binding<SnapshotId> } => [] });
variants!(#[derive(Copy)] Position { Top {} => [], Bottom {} => [], Index { index: PositiveNat } => [] });
record!(SelectionConstraint { groups: Vec<SelectionGroup>, cardinality: Cardinality, distinct_by: Option<ObjectProperty>, disjoint: Disjointness, aggregate_bounds: Vec<AggregateBound> });
record!(SelectionGroup {
    selector: Selector,
    cardinality: Cardinality
});
record!(AggregateBound { property: NumericProperty, comparison: Comparison, value: Expr<Int> });
tags!(ObjectProperty {
    Names,
    Kind,
    Traits,
    Class,
    Title,
    Cost,
    Attack,
    Life,
    Zone,
    Owner,
    Controller,
    Posture
});

variants!(IntOperation {
    Arithmetic { operation: Arithmetic, arguments: Vec<Expr<Int>> } => ["value.signed-arithmetic"],
    FromNat { value: Expr<Nat> } => ["value.nat-to-int"],
    InformationOrZero { information: NumberRead } => ["value.absent-number-zero"],
    Sum { objects: Selector, property: NumericProperty } => ["value.aggregate.sum"],
    Conditional { condition: Expr<Bool>, then_value: Expr<Int>, else_value: Expr<Int> } => ["value.conditional"]
});
variants!(NatOperation {
    ReceiptActualSum { receipts: MeasuredReceiptList } => ["read.receipt.actual-sum"],
    Conditional { condition: Expr<Bool>, then_value: Expr<Nat>, else_value: Expr<Nat> } => ["value.conditional"],
    CheckedInt { value: Expr<Int> } => ["value.checked-nat"],
    Count { objects: Selector } => ["value.aggregate.count"],
    DistinctCount { objects: Selector, property: ObjectProperty } => ["value.aggregate.distinct-count"]
});
variants!(Predicate {
    EventCause { event: EventContext, pattern: CausePattern } => ["predicate.event-cause"],
    Equal { operands: Box<ValueEquality> } => ["predicate.equal"],
    PlayerEquals { left: Expr<PlayerRef>, right: Expr<PlayerRef> } => ["predicate.player-equals"],
    PlayerIn { player: Expr<PlayerRef>, players: PlayerSet } => ["predicate.player-in"],
    ReceiptStatusEquals { left: Expr<ReceiptStatus>, right: Expr<ReceiptStatus> } => ["predicate.receipt-status"],
    PlayedCard { subject: Expr<PlaySubject>, card: Binding<ObjectRef>, predicate: Expr<Bool> } => ["predicate.played-card"],
    PlayedAbility { subject: Expr<PlaySubject>, ability: Binding<RuntimeAbilityReference>, predicate: Expr<Bool> } => ["predicate.played-ability"],
    HolderIsPlayer { holder: Expr<Holder>, player: Binding<PlayerRef>, predicate: Expr<Bool> } => ["predicate.holder-player"],
    HolderIsObject { holder: Expr<Holder>, object: Binding<ObjectRef>, predicate: Expr<Bool> } => ["predicate.holder-object"],
    AnyStatFacet { facets: Expr<NonEmpty<StatDelta>>, facet: Binding<StatDelta>, property: Stat, predicate: Expr<Bool> } => ["predicate.stat-facet"],
    AnySnapshot { snapshots: Expr<Vec<RuntimeObjectView>>, object: Binding<RuntimeObjectView>, predicate: Expr<Bool> } => ["predicate.snapshot-any"],
    AnyMove { moves: Expr<Vec<MoveResult>>, movement: Binding<MoveResult>, predicate: Expr<Bool> } => ["predicate.move-any"],
    MoveSucceeded { movement: Binding<MoveResult> } => ["read.receipt.move-succeeded"],
    ReceiptAny { receipt: ObjectReceipt, projection: MemberProjection, member: Binding<RuntimeObjectView>, predicate: Expr<Bool> } => ["read.receipt.member-any"],
    ObjectEquals { left: Expr<ObjectRef>, right: Expr<ObjectRef> } => ["predicate.object-equals"],
    RecipientIsLeader { recipient: DamageRecipient, players: PlayerSet } => ["predicate.recipient-leader"],
    RecipientIsFollower { recipient: DamageRecipient, objects: Selector } => ["predicate.recipient-follower"],
    ZoneEquals { left: Expr<ZoneRef>, right: Expr<ZoneRef> } => ["predicate.zone-equals"],
    PostureEquals { left: Expr<Posture>, right: Expr<Posture> } => ["predicate.posture-equals"],
    EvolutionEquals { left: Expr<EvolutionState>, right: Expr<EvolutionState> } => ["predicate.evolution-equals"],
    PhaseEquals { left: Expr<Phase>, right: Expr<Phase> } => ["predicate.phase-equals"],
    TitleEquals { left: Expr<Info<TitleId>>, right: Expr<Info<TitleId>> } => ["predicate.title-equals"],
    HasAbility { abilities: Expr<Vec<AbilityReference>>, predicate: Box<AbilityPredicate> } => ["predicate.has-ability"],
    HasTrait { traits: Expr<Vec<TraitId>>, trait_id: TraitId } => ["predicate.has-trait"],
    NamesIntersect { left: Expr<Vec<CardName>>, right: Expr<Vec<CardName>> } => ["predicate.names-intersect"],
    KindEquals { left: Expr<CardKind>, right: Expr<CardKind> } => ["predicate.kind-equals"],
    ClassEquals { left: Expr<Info<ClassId>>, right: Expr<Info<ClassId>> } => ["predicate.class-equals"],
    And { predicates: Vec<Expr<Bool>> } => ["predicate.and"],
    Or { predicates: Vec<Expr<Bool>> } => ["predicate.or"],
    Not { predicate: Expr<Bool> } => ["predicate.not"],
    CompareInt { left: Expr<Int>, comparison: Comparison, right: Expr<Int> } => ["predicate.compare-int"],
    CompareNat { left: Expr<Nat>, comparison: Comparison, right: Expr<Nat> } => ["predicate.compare-nat"],
    HasInfo { information: NumberRead } => ["predicate.has-info"],
    NameEquals { object: ObjectView, name: Expr<CardName> } => ["predicate.name-equals"],
    NameContains { object: ObjectView, name: Expr<CardName> } => ["predicate.name-contains"],
    MemberOf { object: ObjectOperand, set: Selector } => ["predicate.member-of"],
    Exists { objects: Selector } => ["predicate.exists"],
    All { objects: Selector, binding: Binding<ObjectRef>, predicate: Expr<Bool> } => ["predicate.all"],
    Any { objects: Selector, binding: Binding<ObjectRef>, predicate: Expr<Bool> } => ["predicate.any"]
});
variants!(NameOperation { UniqueName { object: ObjectView } => ["value.unique-name"] });
variants!(ZoneOperation { OfPlayer { player: PlayerRef, kind: ZoneKind } => [] });
tags!(NumericProperty { Cost, Attack, Life });
variants!(NumberRead {
    Cost { object: ObjectView, view: CharacteristicView } => ["read.cost"],
    Stats { object: ObjectView, property: Stat, view: CharacteristicView } => ["read.stats"]
});
variants!(IntRead {
    PlayerResource { player: PlayerRef, property: PlayerResource } => ["read.player-resource"],
    HistorySum { query: NumericHistoryQuery } => ["read.history.sum"]
});
tags!(PlayerResource {
    Life,
    PpCurrent,
    PpMaximum,
    Ep,
    Sep
});
variants!(NatRead {
    Counter { holder: Holder, kind: CounterKind } => ["read.counter"],
    ZoneCount { selector: Selector } => ["read.zone-count"],
    ZoneCapacity { player: PlayerRef, zone: CapacityZone, property: CapacityProperty } => ["read.zone-capacity"],
    Combo { player: PlayerRef, execution: ExecutionRef } => ["read.combo"],
    HistoryCount { query: HistoryQuery, count: HistoryCount } => ["read.history.count"],
    TurnOrdinal { player: PlayerRef } => ["read.turn.ordinal"]
});
tags!(CapacityZone { Field, Ex });
tags!(CapacityProperty { Capacity, Free });
tags!(SpellCountTime {
    AtResolution,
    AtPlay
});
variants!(BoolRead {
    Awakening { player: PlayerRef } => ["read.awakening"],
    State { object: ObjectView, property: BooleanState } => ["read.state"],
    HistoryExists { query: HistoryQuery } => ["read.history.exists"],
    OwnTurn { player: PlayerRef } => ["read.turn.own"],
    LegalAttack { attacker: ObjectOperand, ignore_querying_clause: EffectDefinitionId } => ["read.legal-attack"]
});
tags!(BooleanState {
    TokenStatus,
    FaceUp,
    Boxed,
    Racing,
    AdvanceFollower
});
variants!(NameRead { CollaborationName { object: ObjectView } => ["read.collaboration-name"] });
variants!(ZoneRead { Zone { object: ObjectView } => ["read.zone"] });

tags!(MemberProjection { Before, After });
variants!(Holder { FromExpression { value: Box<Expr<Holder>> } => [], Object { object: ObjectOperand } => [], Player { player: PlayerRef } => [] });

tags!(KeywordId {
    Evolution,
    AdvanceActivate,
    Fusion,
    Fanfare,
    Lastword,
    OnEvolve,
    OnAttack,
    OnSuperEvolve,
    Quick,
    Ward,
    Storm,
    Rush,
    Assail,
    Intimidate,
    Aura,
    Drain,
    Bane,
    Combo,
    SpellChain,
    Stack,
    EarthRite,
    Awakening,
    NecroCharge,
    Crimson,
    Meal,
    OnRace,
    Lesson,
    StartAmulet,
    SingleDrive,
    TwinDrive,
    Drive,
    OnDriveGain,
    Ride,
    UnionBurst
});

/// Uninhabited: a domain without this operation cannot construct a fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoRead {}
impl Walk for NoRead {
    #[expect(
        clippy::uninhabited_references,
        reason = "Value requires Walk for every associated domain; safe Rust cannot construct this domain or call its walker."
    )]
    fn walk(&self, _visitor: &mut Visitor, _path: &str) {
        match *self {}
    }
}
variants!(IntFrozen {
    StatBefore { facet: Binding<StatDelta> } => ["read.event-field"],
    StatAfter { facet: Binding<StatDelta> } => ["read.event-field"],
    LifeChangeBefore { event: Binding<LifeChangeEvent> } => ["read.event-field"],
    LifeChangeAfter { event: Binding<LifeChangeEvent> } => ["read.event-field"],
    LifeChangeDelta { event: Binding<LifeChangeEvent> } => ["read.event-field"],
    CounterChangeDelta { event: Binding<CounterChangeEvent> } => ["read.event-field"],
    ReceiptRequested { receipt: MeasuredReceipt } => ["read.receipt.requested"],
    DamageAmount { event: Binding<DamageEvent> } => ["read.event-field"],
    StatDelta { event: Binding<StatDelta> } => ["read.event-field"]
});
variants!(NatFrozen {
    ReceiptActual { receipt: MeasuredReceipt } => ["read.receipt.actual"],
    EarthRiteRemovedStack { component: Binding<EarthRitePaidComponent> } => ["read.payment.earth-rite-stack"],
    DiceAcceptedAttempt { event: Binding<DiceEvent> } => ["read.event-field"],
    TargetedTargetSlot { event: Binding<TargetedEvent> } => ["read.event-field"],
    DrawOrdinal { event: Binding<DrawEvent> } => ["read.event-field"],
    CounterChangeBefore { event: Binding<CounterChangeEvent> } => ["read.event-field"],
    CounterChangeAfter { event: Binding<CounterChangeEvent> } => ["read.event-field"],
    AttackOrdinal { event: Binding<AttackEvent> } => ["read.event-field"],
    RaceOrdinal { event: Binding<RaceEvent> } => ["read.event-field"],
    SpellChain { player: PlayerRef, execution: ExecutionRef, at: SpellCountTime } => ["read.spell-chain", "value.capture"],
    NecroCharge { player: PlayerRef, execution: ExecutionRef, at: SpellCountTime } => ["read.necro-charge", "value.capture"],
    DiceValue { event: Binding<DiceEvent> } => ["read.event-field"]
});
variants!(BoolFrozen {
    PaymentsComplete { receipts: Binding<ReceiptList<PaymentResult>>, quantifier: ReceiptQuantifier } => ["read.receipt.payments-complete"],
    PaymentZero { event: Binding<PaymentEvent> } => ["read.event-field"],
    PaymentReplaced { event: Binding<PaymentEvent> } => ["read.event-field"],
    DriveGainedFirstGain { event: Binding<DriveGainedEvent> } => ["read.event-field"],
    DriveCheckedTriggerAccepted { event: Binding<DriveCheckedEvent> } => ["read.event-field"],
    PaymentComplete { receipt: PaymentReference } => ["read.receipt.payment-complete"]
});
variants!(#[derive(Copy)] ZoneFrozen {
    PlayOriginZone { event: Binding<PlayEvent> } => ["read.event-field"],
    LeaveFieldTo { event: Binding<LeaveFieldEvent> } => ["read.event-field"],
    DiscardActualTo { event: Binding<DiscardEvent> } => ["read.event-field"],
    BanishFrom { event: Binding<BanishEvent> } => ["read.event-field"],
    DriveCheckedFinalDestination { event: Binding<DriveCheckedEvent> } => ["read.event-field"],
    MoveFrom { event: Binding<MoveEvent> } => ["read.event-field"],
    MoveTo { event: Binding<MoveEvent> } => ["read.event-field"]
});
variants!(ObjectFrozen {
    SnapshotObject { snapshot: Expr<RuntimeObjectView> } => ["read.snapshot-object"],
    SourceHolder {} => ["reference.source-holder"],
    EnterFieldObject { event: Binding<EnterFieldEvent> } => ["read.event-field"],
    TargetedObject { event: Binding<TargetedEvent> } => ["read.event-field"],
    LeaveFieldObjectBefore { event: Binding<LeaveFieldEvent> } => ["read.event-field"],
    EnterExObject { event: Binding<EnterExEvent> } => ["read.event-field"],
    DrawObject { event: Binding<DrawEvent> } => ["read.event-field"],
    DiscardCardBefore { event: Binding<DiscardEvent> } => ["read.event-field"],
    DestroyObjectBefore { event: Binding<DestroyEvent> } => ["read.event-field"],
    BanishCardBefore { event: Binding<BanishEvent> } => ["read.event-field"],
    PostureObject { event: Binding<PostureEvent> } => ["read.event-field"],
    EvolveSubject { event: Binding<EvolveEvent> } => ["read.event-field"],
    EvolveWith { event: Binding<EvolveEvent> } => ["read.event-field"],
    AttackAttacker { event: Binding<AttackEvent> } => ["read.event-field"],
    RaceSubject { event: Binding<RaceEvent> } => ["read.event-field"],
    DriveGainedSubject { event: Binding<DriveGainedEvent> } => ["read.event-field"],
    DriveTriggerCheckedCard { event: Binding<DriveTriggerEvent> } => ["read.event-field"],
    UbActivatedHolder { event: Binding<UbActivatedEvent> } => ["read.event-field"],
    FusedSubjectBefore { event: Binding<FusedEvent> } => ["read.event-field"],
    FusedSubjectAfter { event: Binding<FusedEvent> } => ["read.event-field"],
    EquipHolder { event: Binding<EquipEvent> } => ["read.event-field"],
    EquipEquipment { event: Binding<EquipEvent> } => ["read.event-field"],
    DriveCheckedCheckedCard { event: Binding<DriveCheckedEvent> } => ["read.event-field"],
    MoveBefore { event: Binding<MoveEvent> } => ["read.event-field"],
    MoveAfter { event: Binding<MoveEvent> } => ["read.event-field"]
});
variants!(NamesRead { Names { object: ObjectView } => ["read.name"] });
variants!(KindRead { Kind { object: ObjectView, view: CharacteristicView } => ["read.kind"] });
variants!(TraitsRead { Traits { object: ObjectView, view: CharacteristicView } => ["read.traits"] });
variants!(ClassRead { Class { object: ObjectView, view: CharacteristicView } => ["read.affiliation.class"] });
variants!(TitleRead { Title { object: ObjectView, view: CharacteristicView } => ["read.affiliation.title"] });
variants!(PlayerRead { Role { object: ObjectView, role: Role } => ["read.roles"], Active {} => ["read.turn.active"] });
variants!(PostureRead { Posture { object: ObjectView } => ["read.state.posture"] });
variants!(EvolutionRead { Evolution { object: ObjectView } => ["read.state.evolution"] });
variants!(AbilitiesRead { Abilities { object: ObjectView, view: CharacteristicView } => ["read.abilities"] });
variants!(#[derive(Copy)] TurnRead { Turn {} => ["read.turn.id"] });
variants!(#[derive(Copy)] PhaseRead { Phase {} => ["read.turn.phase"] });
variants!(#[derive(Copy)] EntryPaymentRead { EntryPayment { event: Binding<EnterFieldEvent> } => ["read.entry-payment"]
});
domain!(Vec<CardName>, NamesRead, NoRead, NamesFrozen, Vec<CardName>);
domain!(CardKind, KindRead, NoRead, KindFrozen, CardKind);
domain!(Vec<TraitId>, TraitsRead, NoRead, TraitsFrozen, Vec<TraitId>);
domain!(Info<ClassId>, ClassRead, NoRead, ClassFrozen, Info<ClassId>);
domain!(Info<TitleId>, TitleRead, NoRead, TitleFrozen, Info<TitleId>);
domain!(Info<Int>, NumberRead, NoRead, NumberFrozen, Info<Int>);
domain!(PlayerRef, PlayerRead, NoRead, PlayerFrozen, PlayerRef);
domain!(Posture, PostureRead, NoRead, PostureFields, Posture);
domain!(
    EvolutionState,
    EvolutionRead,
    NoRead,
    EvolutionStateFields,
    EvolutionState
);
domain!(Vec<AbilityReference>, AbilitiesRead, NoRead, NoRead, NoRead);
domain!(TurnId, TurnRead, NoRead, TurnIdFields, NoRead);
domain!(Phase, PhaseRead, NoRead, PhaseFields, Phase);
domain!(ObjectRef, NoRead, NoRead, ObjectFrozen, NoRead);
domain!(
    Info<PaymentReceipt>,
    NoRead,
    NoRead,
    EntryPaymentRead,
    NoRead
);

/// Positive positional domain, distinct from general Nat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveNat(pub NonZeroU64);
impl Walk for PositiveNat {
    fn walk(&self, _visitor: &mut Visitor, _path: &str) {}
}
variants!(#[derive(Copy)] ExecutionRef { Current {} => [], Bound { binding: Binding<ExecutionId> } => [] });
variants!(#[derive(Copy)] EventContext { Trigger {} => [], Bound { binding: Binding<Event> } => [],
    Move { event: Binding<MoveEvent> } => [],
    Damage { event: Binding<DamageEvent> } => [],
    Dice { event: Binding<DiceEvent> } => [],
    EnterField { event: Binding<EnterFieldEvent> } => [],
    Play { event: Binding<PlayEvent> } => [],
    Targeted { event: Binding<TargetedEvent> } => [],
    Resolve { event: Binding<ResolveEvent> } => [],
    Payment { event: Binding<PaymentEvent> } => [],
    LeaveField { event: Binding<LeaveFieldEvent> } => [],
    EnterEx { event: Binding<EnterExEvent> } => [],
    Draw { event: Binding<DrawEvent> } => [],
    Discard { event: Binding<DiscardEvent> } => [],
    Destroy { event: Binding<DestroyEvent> } => [],
    Banish { event: Binding<BanishEvent> } => [],
    StatChange { event: Binding<StatChangeEvent> } => [],
    LifeChange { event: Binding<LifeChangeEvent> } => [],
    CounterChange { event: Binding<CounterChangeEvent> } => [],
    Posture { event: Binding<PostureEvent> } => [],
    Evolve { event: Binding<EvolveEvent> } => [],
    Attack { event: Binding<AttackEvent> } => [],
    Phase { event: Binding<PhaseEvent> } => [],
    Race { event: Binding<RaceEvent> } => [],
    DriveGained { event: Binding<DriveGainedEvent> } => [],
    DriveTrigger { event: Binding<DriveTriggerEvent> } => [],
    UbActivated { event: Binding<UbActivatedEvent> } => [],
    Fused { event: Binding<FusedEvent> } => [],
    Equip { event: Binding<EquipEvent> } => [],
    DriveChecked { event: Binding<DriveCheckedEvent> } => []
});
variants!(#[derive(Copy)] TimeAnchor { Installation {} => [], Bound { binding: Binding<AnchorId> } => [] });
variants!(#[derive(Copy)] ScheduleAnchor { NextTurn {} => [], Bound { binding: Binding<AnchorId> } => [] });
variants!(#[derive(Copy)] HistoryAnchor { Bound { binding: Binding<AnchorId> } => [] });
variants!(#[derive(Copy)] BatchPolicy { Instruction {} => [], Simultaneous { group: InstructionId } => [] });
tags!(RandomnessPolicy { FreshRequest });
tags!(SelectionSnapshot { AtDecision });
tags!(CardSnapshot {
    CurrentVerifiedSnapshot
});
tags!(AbilityReferenceKind { Current, Printed });
tags!(UsageReference {
    CurrentRulePermission
});
variants!(CausePattern {
    Any {} => [],
    Effect { source: EffectSource } => [],
    Rule { rule: RuleId } => [],
    AnyRule {} => [],
    PlayerAction {} => []
});
variants!(EffectSource { Any {} => [], Matching { predicate: Box<SourcePredicate> } => ["predicate.effect-source"] });

variants!(#[derive(Copy)] PlayerFrozen {
    HeaderActor { event: EventContext } => ["read.event-header"],
    DiceRoller { event: Binding<DiceEvent> } => ["read.event-field"],
    EnterFieldController { event: Binding<EnterFieldEvent> } => ["read.event-field"],
    PlayController { event: Binding<PlayEvent> } => ["read.event-field"],
    TargetedChooser { event: Binding<TargetedEvent> } => ["read.event-field"],
    LeaveFieldController { event: Binding<LeaveFieldEvent> } => ["read.event-field"],
    EnterExController { event: Binding<EnterExEvent> } => ["read.event-field"],
    DrawPlayer { event: Binding<DrawEvent> } => ["read.event-field"],
    DiscardPlayer { event: Binding<DiscardEvent> } => ["read.event-field"],
    DestroyController { event: Binding<DestroyEvent> } => ["read.event-field"],
    BanishActor { event: Binding<BanishEvent> } => ["read.event-field"],
    LifeChangeSubject { event: Binding<LifeChangeEvent> } => ["read.event-field"],
    AttackController { event: Binding<AttackEvent> } => ["read.event-field"],
    PhasePlayer { event: Binding<PhaseEvent> } => ["read.event-field"],
    RaceController { event: Binding<RaceEvent> } => ["read.event-field"],
    DriveGainedController { event: Binding<DriveGainedEvent> } => ["read.event-field"],
    DriveTriggerPlayer { event: Binding<DriveTriggerEvent> } => ["read.event-field"],
    UbActivatedController { event: Binding<UbActivatedEvent> } => ["read.event-field"],
    DriveCheckedPlayer { event: Binding<DriveCheckedEvent> } => ["read.event-field"],
    DamageController { event: Binding<DamageEvent> } => ["read.event-field"]
});
variants!(#[derive(Copy)] ReceiptStatusRead { Status { receipt: ReceiptId } => ["read.receipt.status"] });
variants!(ReceiptMovesRead { Moves { receipt: MovementReceipt } => ["read.receipt.moves"] });
domain!(
    ReceiptStatus,
    NoRead,
    NoRead,
    ReceiptStatusRead,
    ReceiptStatus
);
domain!(Vec<MoveResult>, NoRead, NoRead, ReceiptMovesRead, NoRead);

variants!(#[derive(Copy)] EffectReference { CurrentInstallation {} => [], Bound { binding: Binding<EffectId> } => [] });

tags!(SliceDirection { Forward, Reverse });
tags!(Disjointness {
    MayOverlap,
    Disjoint
});
tags!(GenerationDeduplication {
    ByObjectAndGeneration
});

variants!(PaymentReference {
    Local { receipt: ReceiptReference<PaymentResult> } => [],
    Entry { receipt: Expr<Info<PaymentReceipt>>, absent: AbsentPayment } => []
});
tags!(AbsentPayment { NotComplete });

variants!(#[derive(Copy)] MovementEvent {
    Move { event: Binding<MoveEvent> } => [],
    LeaveField { event: Binding<LeaveFieldEvent> } => []
});

variants!(NamesFrozen {
    Snapshot { object: Expr<RuntimeObjectView> } => ["read.snapshot-characteristics"],
    Characters { characters: Expr<Characteristics> } => ["read.event-characteristics"]
});

variants!(KindFrozen {
    Snapshot { object: Expr<RuntimeObjectView> } => ["read.snapshot-characteristics"],
    Characters { characters: Expr<Characteristics> } => ["read.event-characteristics"]
});

variants!(TraitsFrozen {
    Snapshot { object: Expr<RuntimeObjectView> } => ["read.snapshot-characteristics"],
    Characters { characters: Expr<Characteristics> } => ["read.event-characteristics"]
});

variants!(ClassFrozen {
    Snapshot { object: Expr<RuntimeObjectView> } => ["read.snapshot-characteristics"],
    Characters { characters: Expr<Characteristics> } => ["read.event-characteristics"]
});

variants!(TitleFrozen {
    Snapshot { object: Expr<RuntimeObjectView> } => ["read.snapshot-characteristics"],
    Characters { characters: Expr<Characteristics> } => ["read.event-characteristics"]
});

variants!(NumberFrozen {
    Snapshot { object: Expr<RuntimeObjectView>, property: NumericProperty } => ["read.snapshot-characteristics"],
    Characters { characters: Expr<Characteristics>, property: NumericProperty } => ["read.event-characteristics"]
});

tags!(ReceiptQuantifier { All, Any });
