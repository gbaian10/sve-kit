use super::{
    AbilityId, AbilityReferenceKind, Action, BanishOutcome, BatchOutcome, Binding, Bool,
    CaptureRef, CardKind, CardName, CardQuantityOutcome, CardSnapshot, Cardinality, ClassId,
    CounterQuantityOutcome, DamageAllocation, DamageQuantityOutcome, DamageRecipient, DecisionId,
    DestroyOutcome, DiscardOutcome, Effect, EffectReference, EventContext, EventSnapshotPolicy,
    EvolutionState, ExecutionRef, Expr, FaceId, FlipOutcome, FusionOutcome, Info, InstructionId,
    Int, KeywordId, LookOutcome, ModeId, MoveOutcome, MoveResult, MovementEvent, Nat,
    ObjectOperand, ObjectRef, ObjectView, OrderOutcome, PaymentReceipt, PaymentResult, Phase,
    PlayerRef, PlayerSet, Posture, ReceiptId, ReceiptList, RecipientSet, RuleId, ScopeId,
    SearchOutcome, SelectionConstraint, SelectionSnapshot, Selector, TitleId, TokenOutcome,
    TraitId, TransformOutcome, TriggerSpec, TurnId, Visitor, Walk, ZoneKind, ZoneRef,
};
use core::mem::replace;

/// Publishable source coordinates only; never a filesystem path or card text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceSpan {
    /// Snapshot card identifier.
    pub card_id: String,
    /// Original official number.
    pub card_no: String,
    /// Ability scope within the card.
    pub scope: String,
    /// Card or token owner coordinate.
    pub owner: String,
    /// Face coordinate.
    pub face: String,
    /// Source field, for example text.
    pub field: String,
    /// Optional section coordinate.
    pub section: Option<String>,
    /// Source line coordinate.
    pub line: u32,
    /// Fragment coordinate.
    pub fragment: Option<String>,
    /// Author node path.
    pub author_path: String,
}
/// A tree node retains its own author coordinates through expansion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located<T> {
    /// Fully expanded node.
    pub node: T,
    /// Publishable provenance.
    pub source: SourceSpan,
}
impl<T: Walk> Walk for Located<T> {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        let previous = replace(&mut visitor.source, self.source.clone());
        self.node.walk(visitor, path);
        visitor.source = previous;
    }
}
record!(Program { scope: ScopeId, steps: Vec<Located<Node>> });
variants!(Node {
    Instruction { instruction: Box<Instruction> } => [],
    Control { control: Control } => [],
    Capture { capture: Capture } => ["value.capture"],
    Decision { decision: Decision } => ["decision.input"],
});
record!(Instruction { id: InstructionId, actor: PlayerRef, source: AbilitySource, precondition: Option<Precondition>, context: InstructionContext, receipt: ReceiptId, action: Action });
tags!(InstructionContext { Effect, Cost });
record!(AbilitySource {
    holder: ObjectOperand,
    origin: AbilityOrigin,
    controller: PlayerRef
});
variants!(AbilityOrigin {
    Printed { face: FaceId, line: Nat } => [],
    Granted { effect: EffectReference, grant: AbilityId, provider: TextProvider } => [],
    Keyword { keyword: KeywordId, origin: Box<AbilityOrigin> } => []
});
record!(TextProvider {
    provider: ObjectView,
    face: FaceId,
    ability: AbilityId
});
record!(AbilityReference {
    holder: ObjectOperand,
    instance: AbilityReferenceKind,
    definition: AbilityId
});
record!(Ability { id: AbilityId, scope: ScopeId, source: AbilitySource, kind: AbilityKind, qualification: AbilityQualification, active_zones: Vec<ZoneKind>, event_captures: Vec<Capture>, play: Option<PlaySpec>, trigger: Option<TriggerSpec>, body: Option<Program>, declarations: Vec<Effect> });
tags!(AbilityKind {
    Spell,
    Activated,
    Triggered,
    Static
});
record!(PlaySpec { captures: Vec<Capture>, parameters: Vec<Decision>, costs: CostSpec });
record!(Precondition { kind: PreconditionKind, predicate: Expr<Bool>, protects: InstructionId, reason: PreconditionReason });
tags!(PreconditionKind {
    CardTextPrerequisite,
    ForcedZeroUnits
});
variants!(#[derive(Copy)] PreconditionReason { CardText {} => [], Rule { rule: RuleId } => [] });
record!(Decision {
    id: DecisionId,
    captures: Vec<Capture>,
    actor: PlayerRef,
    domain: ChoiceDomain,
    kind: DecisionKind,
    visibility: Visibility
});
variants!(Procedure {
    Rule { scope: ScopeId, procedure: RuleProcedure, rule: RuleId, decision: Decision } => [],
    Quick { window: QuickWindow, decision: Decision } => []
});
variants!(DecisionKind {
    PlayParameter { slot: PlaySlot, selection_snapshot: SelectionSnapshot, play_scope: ExecutionRef, input: InputMode } => ["decision.play-parameter"],
    ResolutionChoice { may: Declinable, cardinality: Cardinality, precondition: Option<Precondition> } => ["decision.resolution-choice"],
    RuleChoice { procedure: RuleProcedure, rule: RuleId } => ["decision.rule-choice"],
    QuickWindow { window: QuickWindow } => ["decision.quick-window"]
});
tags!(PlaySlot {
    Targets,
    CostMaterials,
    Modes,
    X,
    Distribution,
    OptionalCost,
    Face
});
tags!(InputMode { Inline, Handoff });
tags!(RuleProcedure {
    PendingOrder,
    Capacity,
    GuardAct,
    EndDiscard,
    ReplacementOrder,
    TriggerAllocation,
    EvolutionCard,
    Opening,
    DeckOrder,
    StackRecipient,
    ContinuousTieOrder
});
tags!(QuickWindow { Attack, EndPhase });
variants!(ChoiceDomain {
    EndPhaseGuardAct { binding: Binding<Selector> } => [],
    Objects { constraint: SelectionConstraint, binding: Binding<Selector>, target_roles: Option<TargetRoleSpec> } => [],
    Modes { modes: Vec<ModeId>, cardinality: Cardinality, binding: Binding<Vec<ModeId>>, exclude_used: Option<ModeExclusion> } => [],
    Order { objects: Selector, binding: Binding<Selector> } => [],
    Distribution { recipients: RecipientSet, total: Expr<Nat>, binding: Binding<Vec<DamageAllocation>> } => [],
    Integer { cardinality: Cardinality, binding: Binding<Nat> } => [],
    Names { snapshot: CardSnapshot, binding: Binding<CardName> } => [],
    Compound { domains: Vec<ChoiceDomain> } => [],
    QuickActionOrPass { player: PlayerRef } => []
});
variants!(Visibility { Public {} => [], Only { players: PlayerSet } => [], AuthorityOnly {} => [] });
variants!(#[derive(Copy)] CaptureTime {
    AtPlay { execution: ExecutionRef } => [], AtResolution { scope: ScopeId } => [],
    AtSelection { decision: DecisionId } => [], AtEvent { event: EventContext, policy: EventSnapshotPolicy } => []
});
variants!(Capture {
    Names { at: CaptureTime, value: Expr<Vec<CardName>>, into: CaptureRef<Vec<CardName>> } => ["value.capture"],
    Kind { at: CaptureTime, value: Expr<CardKind>, into: CaptureRef<CardKind> } => ["value.capture"],
    Traits { at: CaptureTime, value: Expr<Vec<TraitId>>, into: CaptureRef<Vec<TraitId>> } => ["value.capture"],
    Class { at: CaptureTime, value: Expr<Info<ClassId>>, into: CaptureRef<Info<ClassId>> } => ["value.capture"],
    Title { at: CaptureTime, value: Expr<Info<TitleId>>, into: CaptureRef<Info<TitleId>> } => ["value.capture"],
    NumberInfo { at: CaptureTime, value: Expr<Info<Int>>, into: CaptureRef<Info<Int>> } => ["value.capture"],
    Player { at: CaptureTime, value: Expr<PlayerRef>, into: CaptureRef<PlayerRef> } => ["value.capture"],
    Posture { at: CaptureTime, value: Expr<Posture>, into: CaptureRef<Posture> } => ["value.capture"],
    Evolution { at: CaptureTime, value: Expr<EvolutionState>, into: CaptureRef<EvolutionState> } => ["value.capture"],
    Abilities { at: CaptureTime, value: Expr<Vec<AbilityReference>>, into: CaptureRef<Vec<AbilityReference>> } => ["value.capture"],
    Turn { at: CaptureTime, value: Expr<TurnId>, into: CaptureRef<TurnId> } => ["value.capture"],
    Phase { at: CaptureTime, value: Expr<Phase>, into: CaptureRef<Phase> } => ["value.capture"],
    Object { at: CaptureTime, value: Expr<ObjectRef>, into: CaptureRef<ObjectRef> } => ["value.capture"],
    EntryPayment { at: CaptureTime, value: Expr<Info<PaymentReceipt>>, into: CaptureRef<Info<PaymentReceipt>> } => ["value.capture"],
    ReceiptStatus { at: CaptureTime, value: Expr<ReceiptStatus>, into: CaptureRef<ReceiptStatus> } => ["value.capture"],
    ReceiptMoves { at: CaptureTime, value: Expr<Vec<MoveResult>>, into: CaptureRef<Vec<MoveResult>> } => ["value.capture"],

    Integer { at: CaptureTime, value: Expr<Int>, into: CaptureRef<Int> } => ["value.capture"],
    Natural { at: CaptureTime, value: Expr<Nat>, into: CaptureRef<Nat> } => ["value.capture"],
    Boolean { at: CaptureTime, value: Expr<Bool>, into: CaptureRef<Bool> } => ["value.capture"],
    Objects { at: CaptureTime, value: Selector, into: CaptureRef<Selector> } => ["value.capture"],
    Name { at: CaptureTime, value: Expr<CardName>, into: CaptureRef<CardName> } => ["value.capture"],
    Zone { at: CaptureTime, value: Expr<ZoneRef>, into: CaptureRef<ZoneRef> } => ["value.capture"]
});
variants!(Control {
    Simultaneous { batch: InstructionBatch, receipt: ReceiptId } => ["control.simultaneous", "transaction.shared-capacity"],
    Match { value: DamageRecipient, leader: Binding<PlayerRef>, leader_body: Program, follower: Binding<ObjectRef>, follower_body: Program } => ["control.match"],
    Sequence { body: Program } => ["control.sequence"],
    If { condition: Expr<Bool>, then_body: Program, else_body: Option<Program> } => ["control.if"],
    May { body: Program, choice: Decision, receipt: ReceiptId } => ["control.may"],
    Mode { modes: Vec<Mode>, count: Expr<Nat>, chosen: ModeDecision, choice_kind: ModeChoiceKind } => ["control.mode"],
    Repeat { body: RepeatBody, termination: Termination, check: IterationCheck, scope: ScopeId, receipts: Vec<ReceiptExport> } => ["control.repeat"],
    EachPlayer { players: PlayerSet, player: Binding<PlayerRef>, stages: Vec<PlayerStage>, choice_order: PlayerOrder } => ["control.each-player"],
    EachObject { input: Selector, binding: Binding<ObjectRef>, body: Program, order: Option<IterationOrder> } => ["control.each-object"],
    Pay { at: PaymentTime, costs: CostSpec, optional: PaymentOptionality, on_paid: Program, receipt: ReceiptId } => ["control.pay"]
});
record!(Mode {
    id: ModeId,
    parameters: Vec<Decision>,
    body: Program
});
tags!(ModeChoiceKind {
    ChoiceKeyword,
    ResolutionAlternative
});
variants!(RepeatBody { Program { body: Program } => [], BodyReference { ability: AbilityId, first_clause: Nat, last_clause: Nat } => ["control.body-reference"] });
variants!(Termination { Count { count: Expr<Nat> } => [], Until { predicate: Expr<Bool> } => [], While { predicate: Expr<Bool> } => [] });
tags!(IterationCheck {
    BeforeIteration,
    AfterIteration
});
record!(PlayerStage {
    body: Program,
    mode: StageMode
});
variants!(#[derive(Copy)] StageMode { ChoiceProcedure {} => [], NonChoiceSimultaneous {} => [], ExplicitOrder { rule: RuleId } => [] });
tags!(PlayerOrder {
    TurnPlayerFirst,
    FirstPlayerFirst
});
tags!(IterationOrder {
    Printed,
    Zone,
    Chosen
});
tags!(PaymentTime {
    PlayParameter,
    ResolutionChoice
});
// Materials exposes the instruction's one selection; it never asks for a second decision.
record!(CostStep { instruction: Box<Instruction>, materials: Option<Binding<Selector>> });

tags!(QuantityUnit {
    Cards,
    Damage,
    Counters,
    Life,
    Pp,
    Ep,
    Sep,
    Times
});

tags!(ReceiptStatus {
    Applied,
    Partial,
    Declined,
    Inapplicable
});

tags!(ReceiptReason {
    Prevented,
    Prohibited,
    Capacity,
    InvalidGeneration,
    Inapplicable,
    Declined,
    Replaced,
    Exhausted
});

record!(CardProgram { abilities: Vec<Located<Ability>> });
variants!(CostSpec { Ordered { steps: Vec<CostStep> } => [], Alternative { alternatives: Vec<CostSpec> } => [] });

/// Typed reference to one instruction result; lowering proves its defining instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiptReference<T> {
    /// Receipt identity, never an object ID.
    location: ReceiptLocation<T>,
}
impl<T> ReceiptReference<T> {
    /// Program-local result slot; never a physical object identity.
    #[must_use]
    pub const fn location(&self) -> &ReceiptLocation<T> {
        &self.location
    }
    /// Construct an internal reference after checking the defining instruction.
    #[must_use]
    pub const fn new(id: ReceiptId) -> Self {
        Self {
            location: ReceiptLocation::Local { id },
        }
    }
    /// Use a result explicitly exported to the loop's termination condition.
    #[must_use]
    pub const fn iteration(binding: Binding<T>) -> Self {
        Self {
            location: ReceiptLocation::Iteration { binding },
        }
    }
}
impl<T> Walk for ReceiptReference<T> {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        match &self.location {
            ReceiptLocation::Local { .. } => visitor.require("receipt.local", path),
            ReceiptLocation::Iteration { .. } => visitor.require("receipt.iteration", path),
        }
    }
}
/// A local result slot or a typed result exported from the current iteration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReceiptLocation<T> {
    /// Lowering checks the slot's producer and dominance.
    Local {
        /// Program-local slot.
        id: ReceiptId,
    },
    /// Only a matching repeat export can create this binding.
    Iteration {
        /// Current iteration's result.
        binding: Binding<T>,
    },
}

variants!(MovementReceipt {
    TriggerEvent { event: MovementEvent } => ["read.event-header.move-receipt"],
    Move { receipt: ReceiptReference<MoveOutcome> } => [],
    Search { receipt: ReceiptReference<SearchOutcome> } => [],
    Discard { receipt: ReceiptReference<DiscardOutcome> } => [],
    Destroy { receipt: ReceiptReference<DestroyOutcome> } => [],
    Banish { receipt: ReceiptReference<BanishOutcome> } => [],
    Order { receipt: ReceiptReference<OrderOutcome> } => [],
    Transform { receipt: ReceiptReference<TransformOutcome> } => [],
    Fusion { receipt: ReceiptReference<FusionOutcome> } => []
});

variants!(MeasuredReceipt {
    Cards { receipt: ReceiptReference<CardQuantityOutcome> } => [],
    Damage { receipt: ReceiptReference<DamageQuantityOutcome> } => [],
    Counters { receipt: ReceiptReference<CounterQuantityOutcome> } => []
});

tags!(AbilityQualification {
    Ordinary,
    UnionBurst
});

record!(ModeDecision {
    id: DecisionId,
    actor: PlayerRef,
    domain: ChoiceDomain,
    kind: ModeDecisionKind,
    visibility: Visibility
});
variants!(#[derive(Copy)] ModeDecisionKind { PlayParameter { play_scope: ExecutionRef, input: InputMode } => ["decision.play-parameter"], ResolutionChoice { cardinality: Cardinality } => ["decision.resolution-choice"] });
tags!(ModeExclusion {
    CurrentTurnAbilityInstance
});
/// Target roles have semantics even when all cardinalities happen to be zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRoleSpec {
    /// Roles answered together with target selection.
    pub roles: Vec<TargetRole>,
}
impl Walk for TargetRoleSpec {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        visitor.require("decision.target-roles", path);
        self.roles.walk(visitor, path);
    }
}
record!(#[derive(Copy)] TargetRole {
    role: Nat,
    cardinality: RoleCardinality,
    binding: Binding<Selector>
});
variants!(#[derive(Copy)] RoleCardinality {
    Fixed { cardinality: Cardinality } => [],
    IfNonempty { count: Nat } => ["decision.role-if-nonempty"],
    Remainder {} => ["decision.role-remainder"]
});
variants!(ObjectReceipt { Batch { receipt: ReceiptReference<BatchOutcome> } => [], TransformBanished { receipt: ReceiptReference<TransformOutcome> } => [], TransformCreated { receipt: ReceiptReference<TransformOutcome> } => [], Movement { receipt: MovementReceipt } => [], Token { receipt: ReceiptReference<TokenOutcome> } => [], Flip { receipt: ReceiptReference<FlipOutcome> } => [], Look { receipt: ReceiptReference<LookOutcome> } => [] });

tags!(PaymentOptionality { Required, Optional });
tags!(Declinable {
    Required,
    MayDecline
});

variants!(#[derive(Copy)] MovementReceiptList {
    Move { receipts: Binding<ReceiptList<MoveOutcome>> } => [],
    Search { receipts: Binding<ReceiptList<SearchOutcome>> } => [],
    Discard { receipts: Binding<ReceiptList<DiscardOutcome>> } => [],
    Destroy { receipts: Binding<ReceiptList<DestroyOutcome>> } => [],
    Banish { receipts: Binding<ReceiptList<BanishOutcome>> } => [],
    Order { receipts: Binding<ReceiptList<OrderOutcome>> } => [],
    Transform { receipts: Binding<ReceiptList<TransformOutcome>> } => [],
    Fusion { receipts: Binding<ReceiptList<FusionOutcome>> } => []
});

variants!(
    /// Iteration bindings are visible to termination; collected lists only after the loop.
    /// Lowering verifies the producer and these distinct visibility boundaries.
    ReceiptExport {
    Batch { source: ReceiptReference<BatchOutcome>, iteration: Binding<BatchOutcome>, collect: Option<Binding<ReceiptList<BatchOutcome>>> } => ["control.repeat.export"],
    Move { source: ReceiptReference<MoveOutcome>, iteration: Binding<MoveOutcome>, collect: Option<Binding<ReceiptList<MoveOutcome>>> } => ["control.repeat.export"],
    Search { source: ReceiptReference<SearchOutcome>, iteration: Binding<SearchOutcome>, collect: Option<Binding<ReceiptList<SearchOutcome>>> } => ["control.repeat.export"],
    Discard { source: ReceiptReference<DiscardOutcome>, iteration: Binding<DiscardOutcome>, collect: Option<Binding<ReceiptList<DiscardOutcome>>> } => ["control.repeat.export"],
    Destroy { source: ReceiptReference<DestroyOutcome>, iteration: Binding<DestroyOutcome>, collect: Option<Binding<ReceiptList<DestroyOutcome>>> } => ["control.repeat.export"],
    Banish { source: ReceiptReference<BanishOutcome>, iteration: Binding<BanishOutcome>, collect: Option<Binding<ReceiptList<BanishOutcome>>> } => ["control.repeat.export"],
    Order { source: ReceiptReference<OrderOutcome>, iteration: Binding<OrderOutcome>, collect: Option<Binding<ReceiptList<OrderOutcome>>> } => ["control.repeat.export"],
    Transform { source: ReceiptReference<TransformOutcome>, iteration: Binding<TransformOutcome>, collect: Option<Binding<ReceiptList<TransformOutcome>>> } => ["control.repeat.export"],
    Fusion { source: ReceiptReference<FusionOutcome>, iteration: Binding<FusionOutcome>, collect: Option<Binding<ReceiptList<FusionOutcome>>> } => ["control.repeat.export"],
    Token { source: ReceiptReference<TokenOutcome>, iteration: Binding<TokenOutcome>, collect: Option<Binding<ReceiptList<TokenOutcome>>> } => ["control.repeat.export"],
    Flip { source: ReceiptReference<FlipOutcome>, iteration: Binding<FlipOutcome>, collect: Option<Binding<ReceiptList<FlipOutcome>>> } => ["control.repeat.export"],
    Look { source: ReceiptReference<LookOutcome>, iteration: Binding<LookOutcome>, collect: Option<Binding<ReceiptList<LookOutcome>>> } => ["control.repeat.export"],
    CardQuantity { source: ReceiptReference<CardQuantityOutcome>, iteration: Binding<CardQuantityOutcome>, collect: Option<Binding<ReceiptList<CardQuantityOutcome>>> } => ["control.repeat.export"],
    DamageQuantity { source: ReceiptReference<DamageQuantityOutcome>, iteration: Binding<DamageQuantityOutcome>, collect: Option<Binding<ReceiptList<DamageQuantityOutcome>>> } => ["control.repeat.export"],
    CounterQuantity { source: ReceiptReference<CounterQuantityOutcome>, iteration: Binding<CounterQuantityOutcome>, collect: Option<Binding<ReceiptList<CounterQuantityOutcome>>> } => ["control.repeat.export"],
    Payment { source: ReceiptReference<PaymentResult>, iteration: Binding<PaymentResult>, collect: Option<Binding<ReceiptList<PaymentResult>>> } => ["control.repeat.export"]
});

record!(
    /// Child receipts become visible together only after the shared commit.
    ///
    /// Lowering rejects dependencies on sibling receipts or outputs inside the batch.
    /// Choices use the batch-start membership before any member mutates shared capacity.
    InstructionBatch {
        members: Vec<Located<Instruction>>,
        membership: BatchMembership,
        choice_stages: Vec<BatchChoiceStage>,
        commit: BatchCommit,
        event_views: BatchViews
    }
);
tags!(BatchMembership { AtBatchStart });
tags!(BatchCommit {
    SharedCapacityAtomic
});
tags!(BatchViews { BeforeAndAfter });
record!(BatchChoiceStage { decisions: Vec<Decision>, order: PlayerOrder });

variants!(#[derive(Copy)] ObjectReceiptList {
    Token { receipts: Binding<ReceiptList<TokenOutcome>> } => [],
    Flip { receipts: Binding<ReceiptList<FlipOutcome>> } => [],
    Look { receipts: Binding<ReceiptList<LookOutcome>> } => [],
    Batch { receipts: Binding<ReceiptList<BatchOutcome>> } => []
});

variants!(#[derive(Copy)] MeasuredReceiptList {
    Cards { receipts: Binding<ReceiptList<CardQuantityOutcome>> } => [],
    Damage { receipts: Binding<ReceiptList<DamageQuantityOutcome>> } => [],
    Counters { receipts: Binding<ReceiptList<CounterQuantityOutcome>> } => []
});
