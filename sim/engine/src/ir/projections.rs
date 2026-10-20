//! Frozen projections preserve the originating event and its typed payload.
//! A payload binding retains its envelope; header access never guesses a current event.
use super::values::sealed;
use super::{
    AssociationId, AttackEvent, BanishEvent, BatchId, Binding, Bool, Cause, Characteristics,
    CheckId, CollaborationName, CommitId, Completion, CounterChangeEvent, CounterKind, DamageEvent,
    DamageFlags, DamageRecipient, DecisionId, DestroyEvent, DiceEvent, DieFace, DiscardEvent,
    DrawEvent, DriveCheckedEvent, DriveTriggerEvent, Edge, EnterExEvent, EnterFieldEvent,
    EntryMethod, EntryOrigin, EquipEvent, EventContext, EventId, EvolveEvent, ExecutionId, Expr,
    FusedEvent, GameTime, Holder, Info, InstructionId, Int, LeaveFieldEvent, MoveEvent, MoveMethod,
    MoveResult, NoRead, NonEmpty, OriginFactId, PaidComponent, PaymentEvent, PaymentReceipt,
    PaymentResult, PaymentTime, PhaseEvent, PlayEvent, PlaySubject, Position, PostureEvent,
    RaceEvent, ResolveEvent, RollId, RuntimeAbilityReference, RuntimeAbilitySource,
    RuntimeObjectView, RuntimeReceiptId, RuntimeVisibility, SnapshotId, StatChangeEvent,
    StatChangeReason, StatDelta, TargetedEvent, Timestamp, TriggerIcon, TurnId, UbActivatedEvent,
    Value, ZoneKind,
};
macro_rules! projected_domain {
    ($value:ty, $frozen:ty, $literal:ty) => {
        impl sealed::Value for $value {}
        impl Value for $value {
            type Read = NoRead;
            type Operation = NoRead;
            type Frozen = $frozen;
            type Literal = $literal;
        }
    };
}
variants!(#[derive(Copy)] PositionFields {
    MovePosition { event: Binding<MoveEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] MoveMethodFields {
    MoveMethod { event: Binding<MoveEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] OptionZoneKindFields {
    MoveEnteredFrom { event: Binding<MoveEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] SnapshotFields {
    DamageSource { event: Binding<DamageEvent> } => ["read.event-field"],
    LeaveFieldLki { event: Binding<LeaveFieldEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] RecipientFields {
    DamageRecipient { event: Binding<DamageEvent> } => ["read.event-field"],
    AttackTarget { event: Binding<AttackEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] DamageFlagsFields {
    DamageFlags { event: Binding<DamageEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] RollIdFields {
    DiceRoll { event: Binding<DiceEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] VecDieFaceFields {
    DiceAttempts { event: Binding<DiceEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] EntryOriginFields {
    EnterFieldFrom { event: Binding<EnterFieldEvent> } => ["read.event-field"],
    EnterExFrom { event: Binding<EnterExEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] CharacteristicsFields {
    EnterFieldCharacters { event: Binding<EnterFieldEvent> } => ["read.event-field"],
    EnterExCharacters { event: Binding<EnterExEvent> } => ["read.event-field"],
    DiscardCharacters { event: Binding<DiscardEvent> } => ["read.event-field"],
    DestroyCharacters { event: Binding<DestroyEvent> } => ["read.event-field"],
    BanishCharacters { event: Binding<BanishEvent> } => ["read.event-field"],
    EvolveBefore { event: Binding<EvolveEvent> } => ["read.event-field"],
    EvolveAfter { event: Binding<EvolveEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] EntryMethodFields {
    EnterFieldMethod { event: Binding<EnterFieldEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] PlaySubjectFields {
    PlaySubject { event: Binding<PlayEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] RuntimeReceiptIdFields {
    PlayCost { event: Binding<PlayEvent> } => ["read.event-field"],
    DriveTriggerEffectReceipt { event: Binding<DriveTriggerEvent> } => ["read.event-field"],
    FusedPayment { event: Binding<FusedEvent> } => ["read.event-field"],
    EquipCreatedReceipt { event: Binding<EquipEvent> } => ["read.event-field"],
    DriveCheckedReceipt { event: Binding<DriveCheckedEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] ExecutionIdFields {
    TargetedBy { event: Binding<TargetedEvent> } => ["read.event-field"],
    ResolveExecution { event: Binding<ResolveEvent> } => ["read.event-field"],
    UbActivatedExecution { event: Binding<UbActivatedEvent> } => ["read.event-field"],
    FusedExecution { event: Binding<FusedEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] DecisionIdFields {
    TargetedSelection { event: Binding<TargetedEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] RuntimeAbilityReferenceFields {
    ResolveAbility { event: Binding<ResolveEvent> } => ["read.event-field"],
    UbActivatedAbility { event: Binding<UbActivatedEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] CompletionFields {
    ResolveCompletion { event: Binding<ResolveEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] VecRuntimeReceiptIdFields {
    ResolveReceipts { event: Binding<ResolveEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] PaymentResultFields {
    PaymentPayment { event: Binding<PaymentEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] PaidComponentFields {
    PaymentComponent { event: Binding<PaymentEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] PaymentTimeFields {
    PaymentPhase { event: Binding<PaymentEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] InstructionIdFields {
    DrawInstruction { event: Binding<DrawEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] HolderFields {
    StatChangeSubject { event: Binding<StatChangeEvent> } => ["read.event-field"],
    CounterChangeHolder { event: Binding<CounterChangeEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] FacetFields {
    StatChangeFacets { event: Binding<StatChangeEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] StatChangeReasonFields {
    StatChangeReason { event: Binding<StatChangeEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] CommitIdFields {
    StatChangeCommit { event: Binding<StatChangeEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] OptionInstructionIdFields {
    StatChangeInstruction { event: Binding<StatChangeEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] CounterKindFields {
    CounterChangeKind { event: Binding<CounterChangeEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] PostureFields {
    PostureBefore { event: Binding<PostureEvent> } => ["read.event-field"],
    PostureAfter { event: Binding<PostureEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] EvolutionStateFields {
    EvolveKind { event: Binding<EvolveEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] TurnIdFields {
    HeaderTurn { event: EventContext } => ["read.event-header"],
    PhaseTurn { event: Binding<PhaseEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] PhaseFields {
    HeaderPhase { event: EventContext } => ["read.event-header"],
    PhasePhase { event: Binding<PhaseEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] EdgeFields {
    PhaseEdge { event: Binding<PhaseEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] AssociationIdFields {
    RaceLink { event: Binding<RaceEvent> } => ["read.event-field"],
    EquipAssociation { event: Binding<EquipEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] TriggerIconFields {
    DriveTriggerIcon { event: Binding<DriveTriggerEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] CheckIdFields {
    DriveTriggerCheck { event: Binding<DriveTriggerEvent> } => ["read.event-field"],
    DriveCheckedCheck { event: Binding<DriveCheckedEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] OriginFactIdFields {
    HeaderOrigin { event: EventContext } => ["read.event-header"],
    UbActivatedPlayFact { event: Binding<UbActivatedEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] VecRuntimeObjectViewFields {
    FusedMaterials { event: Binding<FusedEvent> } => ["read.event-field"]
});

variants!(#[derive(Copy)] RuntimeAbilitySourceFields {
    EquipSource { event: Binding<EquipEvent> } => ["read.event-field"],
    DriveCheckedHolder { event: Binding<DriveCheckedEvent> } => ["read.event-field"]
});

projected_domain!(Position, PositionFields, Position);

projected_domain!(MoveMethod, MoveMethodFields, MoveMethod);

projected_domain!(Option<ZoneKind>, OptionZoneKindFields, Option<ZoneKind>);

projected_domain!(RuntimeObjectView, SnapshotFields, NoRead);

projected_domain!(DamageRecipient, RecipientFields, NoRead);

projected_domain!(DamageFlags, DamageFlagsFields, DamageFlags);

projected_domain!(RollId, RollIdFields, NoRead);

projected_domain!(Vec<DieFace>, VecDieFaceFields, NoRead);

projected_domain!(EntryOrigin, EntryOriginFields, EntryOrigin);

projected_domain!(Characteristics, CharacteristicsFields, NoRead);

projected_domain!(EntryMethod, EntryMethodFields, EntryMethod);

projected_domain!(PlaySubject, PlaySubjectFields, NoRead);

projected_domain!(RuntimeReceiptId, RuntimeReceiptIdFields, NoRead);

projected_domain!(ExecutionId, ExecutionIdFields, NoRead);

projected_domain!(DecisionId, DecisionIdFields, NoRead);

projected_domain!(
    RuntimeAbilityReference,
    RuntimeAbilityReferenceFields,
    NoRead
);

projected_domain!(Completion, CompletionFields, Completion);

projected_domain!(Vec<RuntimeReceiptId>, VecRuntimeReceiptIdFields, NoRead);

projected_domain!(PaymentResult, PaymentResultFields, NoRead);

projected_domain!(PaidComponent, PaidComponentFields, NoRead);

projected_domain!(PaymentTime, PaymentTimeFields, PaymentTime);

projected_domain!(InstructionId, InstructionIdFields, NoRead);

projected_domain!(Holder, HolderFields, NoRead);

projected_domain!(NonEmpty<StatDelta>, FacetFields, NoRead);

projected_domain!(StatChangeReason, StatChangeReasonFields, StatChangeReason);

projected_domain!(CommitId, CommitIdFields, NoRead);

projected_domain!(Option<InstructionId>, OptionInstructionIdFields, NoRead);

projected_domain!(CounterKind, CounterKindFields, CounterKind);

projected_domain!(Edge, EdgeFields, Edge);

projected_domain!(AssociationId, AssociationIdFields, NoRead);

projected_domain!(TriggerIcon, TriggerIconFields, TriggerIcon);

projected_domain!(CheckId, CheckIdFields, NoRead);

projected_domain!(OriginFactId, OriginFactIdFields, NoRead);

projected_domain!(Vec<RuntimeObjectView>, VecRuntimeObjectViewFields, NoRead);

projected_domain!(RuntimeAbilitySource, RuntimeAbilitySourceFields, NoRead);

variants!(ValueEquality {
    InfoInt { left: Expr<Info<Int>>, right: Expr<Info<Int>> } => [],
    InfoCollaborationName { left: Expr<Info<CollaborationName>>, right: Expr<Info<CollaborationName>> } => [],
    Timestamp { left: Expr<Timestamp>, right: Expr<Timestamp> } => [],
    RuntimeVisibility { left: Expr<RuntimeVisibility>, right: Expr<RuntimeVisibility> } => [],
    OptionRuntimeReceiptId { left: Expr<Option<RuntimeReceiptId>>, right: Expr<Option<RuntimeReceiptId>> } => [],
    SnapshotId { left: Expr<SnapshotId>, right: Expr<SnapshotId> } => [],
    GameTime { left: Expr<GameTime>, right: Expr<GameTime> } => [],
    OptionRuntimeAbilitySource { left: Expr<Option<RuntimeAbilitySource>>, right: Expr<Option<RuntimeAbilitySource>> } => [],
    Cause { left: Expr<Cause>, right: Expr<Cause> } => [],
    OptionExecutionId { left: Expr<Option<ExecutionId>>, right: Expr<Option<ExecutionId>> } => [],
    OptionBatchId { left: Expr<Option<BatchId>>, right: Expr<Option<BatchId>> } => [],
    EventId { left: Expr<EventId>, right: Expr<EventId> } => [],
    AssociationId { left: Expr<AssociationId>, right: Expr<AssociationId> } => [],
    Bool { left: Expr<Bool>, right: Expr<Bool> } => [],
    Characteristics { left: Expr<Characteristics>, right: Expr<Characteristics> } => [],
    CheckId { left: Expr<CheckId>, right: Expr<CheckId> } => [],
    CommitId { left: Expr<CommitId>, right: Expr<CommitId> } => [],
    Completion { left: Expr<Completion>, right: Expr<Completion> } => [],
    CounterKind { left: Expr<CounterKind>, right: Expr<CounterKind> } => [],
    DamageFlags { left: Expr<DamageFlags>, right: Expr<DamageFlags> } => [],
    DamageRecipient { left: Expr<DamageRecipient>, right: Expr<DamageRecipient> } => [],
    DecisionId { left: Expr<DecisionId>, right: Expr<DecisionId> } => [],
    Edge { left: Expr<Edge>, right: Expr<Edge> } => [],
    EntryMethod { left: Expr<EntryMethod>, right: Expr<EntryMethod> } => [],
    EntryOrigin { left: Expr<EntryOrigin>, right: Expr<EntryOrigin> } => [],
    ExecutionId { left: Expr<ExecutionId>, right: Expr<ExecutionId> } => [],
    Holder { left: Expr<Holder>, right: Expr<Holder> } => [],
    InfoPaymentReceipt { left: Expr<Info<PaymentReceipt>>, right: Expr<Info<PaymentReceipt>> } => [],
    InstructionId { left: Expr<InstructionId>, right: Expr<InstructionId> } => [],
    MoveMethod { left: Expr<MoveMethod>, right: Expr<MoveMethod> } => [],
    NonEmptyStatDelta { left: Expr<NonEmpty<StatDelta>>, right: Expr<NonEmpty<StatDelta>> } => [],
    OptionInstructionId { left: Expr<Option<InstructionId>>, right: Expr<Option<InstructionId>> } => [],
    OptionZoneKind { left: Expr<Option<ZoneKind>>, right: Expr<Option<ZoneKind>> } => [],
    OriginFactId { left: Expr<OriginFactId>, right: Expr<OriginFactId> } => [],
    PaidComponent { left: Expr<PaidComponent>, right: Expr<PaidComponent> } => [],
    PaymentResult { left: Expr<PaymentResult>, right: Expr<PaymentResult> } => [],
    PaymentTime { left: Expr<PaymentTime>, right: Expr<PaymentTime> } => [],
    PlaySubject { left: Expr<PlaySubject>, right: Expr<PlaySubject> } => [],
    Position { left: Expr<Position>, right: Expr<Position> } => [],
    RollId { left: Expr<RollId>, right: Expr<RollId> } => [],
    RuntimeAbilityReference { left: Expr<RuntimeAbilityReference>, right: Expr<RuntimeAbilityReference> } => [],
    RuntimeAbilitySource { left: Expr<RuntimeAbilitySource>, right: Expr<RuntimeAbilitySource> } => [],
    RuntimeObjectView { left: Expr<RuntimeObjectView>, right: Expr<RuntimeObjectView> } => [],
    RuntimeReceiptId { left: Expr<RuntimeReceiptId>, right: Expr<RuntimeReceiptId> } => [],
    StatChangeReason { left: Expr<StatChangeReason>, right: Expr<StatChangeReason> } => [],
    TriggerIcon { left: Expr<TriggerIcon>, right: Expr<TriggerIcon> } => [],
    TurnId { left: Expr<TurnId>, right: Expr<TurnId> } => [],
    VecDieFace { left: Expr<Vec<DieFace>>, right: Expr<Vec<DieFace>> } => [],
    VecMoveResult { left: Expr<Vec<MoveResult>>, right: Expr<Vec<MoveResult>> } => [],
    VecRuntimeObjectView { left: Expr<Vec<RuntimeObjectView>>, right: Expr<Vec<RuntimeObjectView>> } => [],
    VecRuntimeReceiptId { left: Expr<Vec<RuntimeReceiptId>>, right: Expr<Vec<RuntimeReceiptId>> } => [],
});

variants!(#[derive(Copy)] EventIdHeaderFields {
    HeaderId { event: EventContext } => ["read.event-header"]
});
projected_domain!(EventId, EventIdHeaderFields, NoRead);

variants!(#[derive(Copy)] OptionBatchIdHeaderFields {
    HeaderBatch { event: EventContext } => ["read.event-header"]
});
projected_domain!(Option<BatchId>, OptionBatchIdHeaderFields, NoRead);

variants!(#[derive(Copy)] OptionExecutionIdHeaderFields {
    HeaderExecution { event: EventContext } => ["read.event-header"]
});
projected_domain!(Option<ExecutionId>, OptionExecutionIdHeaderFields, NoRead);

variants!(#[derive(Copy)] CauseHeaderFields {
    HeaderCause { event: EventContext } => ["read.event-header"]
});
projected_domain!(Cause, CauseHeaderFields, NoRead);

variants!(#[derive(Copy)] OptionRuntimeAbilitySourceHeaderFields {
    HeaderSource { event: EventContext } => ["read.event-header"]
});
projected_domain!(
    Option<RuntimeAbilitySource>,
    OptionRuntimeAbilitySourceHeaderFields,
    NoRead
);

variants!(#[derive(Copy)] GameTimeHeaderFields {
    HeaderTime { event: EventContext } => ["read.event-header"]
});
projected_domain!(GameTime, GameTimeHeaderFields, NoRead);

variants!(#[derive(Copy)] SnapshotIdHeaderFields {
    HeaderBefore { event: EventContext } => ["read.event-header"],
    HeaderAfter { event: EventContext } => ["read.event-header"]
});
projected_domain!(SnapshotId, SnapshotIdHeaderFields, NoRead);

variants!(#[derive(Copy)] OptionRuntimeReceiptIdHeaderFields {
    HeaderReceipt { event: EventContext } => ["read.event-header"]
});
projected_domain!(
    Option<RuntimeReceiptId>,
    OptionRuntimeReceiptIdHeaderFields,
    NoRead
);

variants!(#[derive(Copy)] RuntimeVisibilityHeaderFields {
    HeaderVisibility { event: EventContext } => ["read.event-header"]
});
projected_domain!(RuntimeVisibility, RuntimeVisibilityHeaderFields, NoRead);

variants!(#[derive(Copy)] TimestampHeaderFields {
    HeaderPoint { event: EventContext } => ["read.event-header"]
});
projected_domain!(Timestamp, TimestampHeaderFields, NoRead);
