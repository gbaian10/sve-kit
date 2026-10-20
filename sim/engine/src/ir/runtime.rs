use super::{
    AbilityId, AbilityInstanceId, AssociationId, BatchId, Bool, CardKind, CardName, CheckId,
    ClassId, CommitId, Completion, CounterKind, DamageFlags, DecisionId, DieFace, Edge, EffectId,
    EntryMethod, EntryOrigin, EventId, EventSnapshotPolicy, EvolutionState, ExecutionId, FaceId,
    Generation, Info, InstructionId, Int, ModeId, MoveMethod, Nat, ObjectId, OriginFactId,
    PaymentTime, Phase, Position, PositiveInt, Posture, QuantityUnit, ReceiptId, ReceiptReason,
    ReceiptStatus, ResultMemberId, RollId, RuleId, SnapshotId, Stat, StatChangeReason,
    StatOperation, Timestamp, TitleId, TraitId, TriggerIcon, TurnId, UsageId, ZoneKind,
};

runtime_record!(EventHeader { id: EventId, origin: OriginFactId, batch: Option<BatchId>, execution: Option<ExecutionId>, cause: Cause, actor: PlayerId, source: Option<RuntimeAbilitySource>, time: GameTime, before: SnapshotId, after: SnapshotId, receipt: Option<RuntimeReceiptId>, visibility: RuntimeVisibility });

runtime_record!(
    #[derive(Copy)]
    GameTime {
        turn: TurnId,
        phase: Phase,
        point: Timestamp
    }
);

runtime_variants!(#[derive(Copy)] Cause { Effect { execution: ExecutionId }, Rule { rule: RuleId }, PlayerAction { player: PlayerId } });

runtime_record!(Event {
    header: EventHeader,
    payload: EventPayload
});

runtime_variants!(EventPayload {
    Play { event: PlayEvent },
    Targeted { event: TargetedEvent },
    Resolve { event: ResolveEvent },
    Payment { event: PaymentEvent },
    Move { event: MoveEvent },
    EnterField { event: EnterFieldEvent },
    LeaveField { event: LeaveFieldEvent },
    EnterEx { event: EnterExEvent },
    Draw { event: DrawEvent },
    Discard { event: DiscardEvent },
    Destroy { event: DestroyEvent },
    Banish { event: BanishEvent },
    Damage { event: DamageEvent },
    StatChange { event: StatChangeEvent },
    LifeChange { event: LifeChangeEvent },
    CounterChange { event: CounterChangeEvent },
    Posture { event: PostureEvent },
    Evolve { event: Box<EvolveEvent> },
    Attack { event: AttackEvent },
    Phase { event: PhaseEvent },
    Race { event: RaceEvent },
    DriveGained { event: DriveGainedEvent },
    DriveTrigger { event: DriveTriggerEvent },
    UbActivated { event: UbActivatedEvent },
    Fused { event: FusedEvent },
    Equip { event: EquipEvent },
    Dice { event: DiceEvent },
    DriveChecked { event: DriveCheckedEvent }
});

runtime_variants!(#[derive(Copy)] PlaySubject { Card { object: ObjectRef }, Ability { ability: RuntimeAbilityReference } });

runtime_record!(#[derive(Copy)] MoveEvent { before: ObjectRef, after: ObjectRef, from: RuntimeZoneRef, to: RuntimeZoneRef, position: Position, method: MoveMethod, entered_from: Option<ZoneKind> });

runtime_record!(
    #[derive(Copy)]
    StatDelta {
        property: Stat,
        before: Int,
        after: Int,
        delta: Int,
        operation: StatOperation
    }
);

runtime_record!(Characteristics { names: Vec<CardName>, kind: CardKind, traits: Vec<TraitId>, class: Option<ClassId>, title: Option<TitleId>, cost: Info<Int>, attack: Info<Int>, life: Info<Int>, abilities: Vec<RuntimeAbilityReference>, face: FaceId, evolution: EvolutionState, links: Vec<AssociationId> });

runtime_record!(PendingInstance { ability: AbilityInstanceId, occurrence: Occurrence, controller: PlayerId, quota_reservation: Option<UsageId>, captures: Vec<CapturedValue> });

runtime_variants!(Occurrence { Event { event: EventId, context: Vec<Event> }, State { snapshot: SnapshotId, time: GameTime } });

runtime_record!(SimultaneousBatch { id: BatchId, members: Vec<InstructionInstance>, membership_snapshot: SnapshotId, choice_stages: Vec<DecisionId>, commit_group: CommitId, event_views: EventSnapshotPolicy });

runtime_record!(
    #[derive(Copy)]
    ModeUse {
        execution: ExecutionId,
        ability_instance: AbilityInstanceId,
        mode: ModeId,
        turn: TurnId
    }
);

runtime_record!(
    #[derive(Copy)]
    DamageEvent {
        source: RuntimeObjectView,
        recipient: RuntimeDamageRecipient,
        amount: PositiveInt,
        flags: DamageFlags,
        controller: PlayerId
    }
);

runtime_record!(DiceEvent { roller: PlayerId, roll: RollId, value: DieFace, attempts: Vec<DieFace>, accepted_attempt: Nat });

runtime_record!(EnterFieldEvent { object: ObjectRef, from: EntryOrigin, controller: PlayerId, characters: Characteristics, method: EntryMethod, play_receipt: Option<RuntimeReceiptId> });

runtime_record!(Receipt { id: RuntimeReceiptId, action: InstructionId, requested: Quantity, actual: Quantity, status: ReceiptStatus, successful: Vec<ResultMember>, moves: Vec<MoveResult>, payment: Option<PaymentResult>, reason: Option<ReceiptReason> });

runtime_record!(
    #[derive(Copy)]
    Quantity {
        amount: Int,
        unit: QuantityUnit
    }
);

runtime_record!(#[derive(Copy)] ResultMember { id: ResultMemberId, before: Option<RuntimeObjectView>, after: Option<RuntimeObjectView> });

runtime_record!(MoveResult { before: ObjectRef, after: Option<ObjectRef>, requested_to: RuntimeZoneRef, actual_to: Option<RuntimeZoneRef>, replacements: Vec<EffectId> });

runtime_record!(PaymentResult { complete: Bool, steps: Vec<RuntimeReceiptId> });

runtime_record!(MoveOutcome { moved: Vec<MoveResult> });

runtime_record!(SearchOutcome { found: Vec<RuntimeObjectView>, revealed: Vec<RuntimeObjectView>, moved: Vec<MoveResult> });

runtime_record!(
    #[derive(Copy)]
    CardQuantityOutcome {
        requested: Int,
        actual: Nat
    }
);

runtime_record!(
    #[derive(Copy)]
    DamageQuantityOutcome {
        requested: Int,
        actual: Nat
    }
);

runtime_record!(
    #[derive(Copy)]
    CounterQuantityOutcome {
        requested: Int,
        actual: Nat
    }
);

runtime_record!(
    #[derive(Copy)]
    ObjectRef {
        id: ObjectId,
        generation: Generation
    }
);
runtime_record!(
    #[derive(Copy)]
    PlayerId { seat: u32 }
);
runtime_record!(
    #[derive(Copy)]
    RuntimeReceiptId {
        execution: ExecutionId,
        slot: ReceiptId
    }
);
runtime_record!(
    #[derive(Copy)]
    RuntimeZoneRef {
        player: PlayerId,
        kind: ZoneKind
    }
);
runtime_record!(
    #[derive(Copy)]
    RuntimeObjectView {
        object: ObjectRef,
        snapshot: SnapshotId
    }
);
runtime_record!(
    #[derive(Copy)]
    RuntimeAbilitySource {
        holder: ObjectRef,
        instance: AbilityInstanceId,
        controller: PlayerId,
        definition: AbilityId
    }
);
runtime_record!(
    #[derive(Copy)]
    RuntimeAbilityReference {
        holder: ObjectRef,
        instance: AbilityInstanceId,
        definition: AbilityId
    }
);
runtime_record!(
    #[derive(Copy)]
    InstructionInstance {
        definition: InstructionId,
        execution: ExecutionId
    }
);
runtime_variants!(CapturedValue { Integer { value: Int }, Natural { value: Nat }, Boolean { value: Bool }, Objects { objects: Vec<ObjectRef> }, Name { value: CardName }, Zone { zone: RuntimeZoneRef } });

runtime_variants!(#[derive(Copy)] #[expect(variant_size_differences, reason = "Small identity records stay inline to avoid a heap allocation per reference.")] RuntimeHolder { Object { object: ObjectRef }, Player { player: PlayerId } });
runtime_variants!(#[derive(Copy)] #[expect(variant_size_differences, reason = "Small identity records stay inline to avoid a heap allocation per reference.")] RuntimeDamageRecipient { Object { object: ObjectRef }, Leader { player: PlayerId } });
runtime_variants!(RuntimeVisibility { Public {}, Only { players: Vec<PlayerId> }, AuthorityOnly {} });

runtime_record!(
    #[derive(Copy)]
    PlayEvent {
        subject: PlaySubject,
        controller: PlayerId,
        origin_zone: RuntimeZoneRef,
        cost: RuntimeReceiptId
    }
);
runtime_record!(
    #[derive(Copy)]
    TargetedEvent {
        object: ObjectRef,
        by: ExecutionId,
        selection: DecisionId,
        chooser: PlayerId,
        target_slot: Nat
    }
);
runtime_record!(ResolveEvent { execution: ExecutionId, ability: RuntimeAbilityReference, completion: Completion, receipts: Vec<RuntimeReceiptId> });
runtime_record!(PaymentEvent {
    payment: PaymentResult,
    component: PaidComponent,
    phase: PaymentTime,
    zero: Bool,
    replaced: Bool
});
runtime_record!(
    #[derive(Copy)]
    LeaveFieldEvent {
        object_before: ObjectRef,
        controller: PlayerId,
        lki: RuntimeObjectView,
        to: RuntimeZoneRef
    }
);
runtime_record!(EnterExEvent {
    object: ObjectRef,
    from: EntryOrigin,
    controller: PlayerId,
    characters: Characteristics
});
runtime_record!(
    #[derive(Copy)]
    DrawEvent {
        player: PlayerId,
        object: ObjectRef,
        ordinal: Nat,
        instruction: InstructionId
    }
);
runtime_record!(DiscardEvent {
    player: PlayerId,
    card_before: ObjectRef,
    characters: Characteristics,
    actual_to: RuntimeZoneRef
});
runtime_record!(DestroyEvent {
    object_before: ObjectRef,
    characters: Characteristics,
    controller: PlayerId
});
runtime_record!(BanishEvent {
    card_before: ObjectRef,
    from: RuntimeZoneRef,
    actor: PlayerId,
    characters: Characteristics
});
runtime_record!(StatChangeEvent { subject: RuntimeHolder, facets: NonEmpty<StatDelta>, reason: StatChangeReason, commit: CommitId, instruction: Option<InstructionId> });
runtime_record!(
    #[derive(Copy)]
    LifeChangeEvent {
        subject: PlayerId,
        before: Int,
        after: Int,
        delta: Int
    }
);
runtime_record!(CounterChangeEvent {
    holder: RuntimeHolder,
    kind: CounterKind,
    before: Nat,
    after: Nat,
    delta: Int
});
runtime_record!(
    #[derive(Copy)]
    PostureEvent {
        object: ObjectRef,
        before: Posture,
        after: Posture
    }
);
runtime_record!(EvolveEvent {
    subject: ObjectRef,
    with: ObjectRef,
    kind: EvolutionState,
    before: Characteristics,
    after: Characteristics
});
runtime_record!(
    #[derive(Copy)]
    AttackEvent {
        attacker: ObjectRef,
        target: RuntimeDamageRecipient,
        controller: PlayerId,
        ordinal: Nat
    }
);
runtime_record!(
    #[derive(Copy)]
    PhaseEvent {
        turn: TurnId,
        player: PlayerId,
        phase: Phase,
        edge: Edge
    }
);
runtime_record!(
    #[derive(Copy)]
    RaceEvent {
        subject: ObjectRef,
        controller: PlayerId,
        ordinal: Nat,
        link: AssociationId
    }
);
runtime_record!(
    #[derive(Copy)]
    DriveGainedEvent {
        subject: ObjectRef,
        controller: PlayerId,
        first_gain: Bool
    }
);
runtime_record!(
    #[derive(Copy)]
    DriveTriggerEvent {
        checked_card: ObjectRef,
        player: PlayerId,
        icon: TriggerIcon,
        check: CheckId,
        effect_receipt: RuntimeReceiptId
    }
);
runtime_record!(
    #[derive(Copy)]
    UbActivatedEvent {
        execution: ExecutionId,
        ability: RuntimeAbilityReference,
        holder: ObjectRef,
        controller: PlayerId,
        play_fact: OriginFactId
    }
);
runtime_record!(FusedEvent { execution: ExecutionId, subject_before: ObjectRef, subject_after: ObjectRef, materials: Vec<RuntimeObjectView>, payment: RuntimeReceiptId });
runtime_record!(
    #[derive(Copy)]
    EquipEvent {
        holder: ObjectRef,
        equipment: ObjectRef,
        association: AssociationId,
        source: RuntimeAbilitySource,
        created_receipt: RuntimeReceiptId
    }
);
runtime_record!(
    #[derive(Copy)]
    DriveCheckedEvent {
        check: CheckId,
        player: PlayerId,
        holder: RuntimeAbilitySource,
        checked_card: ObjectRef,
        trigger_accepted: Bool,
        final_destination: RuntimeZoneRef,
        receipt: RuntimeReceiptId
    }
);

runtime_record!(TokenOutcome { created: Vec<ObjectRef> });
runtime_record!(FlipOutcome { successful: Vec<ResultMember> });
runtime_record!(LookOutcome { looked: Vec<RuntimeObjectView> });
/// A fact requiring at least one facet cannot be represented by an empty list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonEmpty<T> {
    /// First required member.
    pub first: T,
    /// Remaining members in order.
    pub rest: Vec<T>,
}

runtime_record!(
    #[derive(Copy)]
    EarthRitePaidComponent { removed_stack: Nat }
);
runtime_variants!(#[derive(Copy)] PaidComponent { General {}, EarthRite { paid: EarthRitePaidComponent } });

runtime_record!(DiscardOutcome { successful: Vec<ResultMember>, moves: Vec<MoveResult> });

runtime_record!(DestroyOutcome { successful: Vec<ResultMember>, moves: Vec<MoveResult> });

runtime_record!(BanishOutcome { successful: Vec<ResultMember>, moves: Vec<MoveResult> });

runtime_record!(OrderOutcome { successful: Vec<ResultMember>, moves: Vec<MoveResult> });

runtime_record!(TransformOutcome { banished: Vec<RuntimeObjectView>, created: Vec<ObjectRef>, moves: Vec<MoveResult> });

runtime_record!(FusionOutcome { successful: Vec<ResultMember>, moves: Vec<MoveResult> });

/// Repetition preserves receipt order and does not deduplicate object generations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptList<T> {
    /// One result per completed iteration.
    pub iterations: Vec<T>,
}
runtime_record!(
    #[derive(Copy)]
    PaymentReceipt {
        id: RuntimeReceiptId
    }
);

runtime_record!(BatchOutcome { receipts: Vec<RuntimeReceiptId>, successful: Vec<ResultMember> });
