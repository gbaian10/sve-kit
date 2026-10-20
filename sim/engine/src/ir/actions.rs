use super::{
    Ability, AbilityPredicate, AbilityReference, AbilitySource, AssociationId, BatchPolicy,
    Binding, Bool, Capture, CardName, CardSnapshot, Cardinality, CharacteristicPatch, CostSpec,
    CounterKind, Decision, DecisionId, Duration, Effect, EffectReference, Expr, FaceId, Holder,
    Int, KeywordId, MovedRef, Nat, ObjectOperand, ObjectRef, ObjectView, PlaySpec, PlayerRef,
    PlayerSet, Position, Posture, Program, RandomnessPolicy, ReceiptId, RollId, RuleId,
    ScheduleAnchor, ScheduleToken, SelectionConstraint, Selector, TextProvider, TimestampBasis,
    TokenTemplateId, TriggerSpec, UsageReference, Visibility, Visitor, Walk, ZoneRef,
};

// I.swap remains rule-only; there is deliberately no constructible variant.
variants!(Action {
    InstallEffect { subjects: InstallationSubjects, capture: InstallationTime, effect: Box<Effect>, duration: Duration } => ["action.install-effect"],
    Move { objects: Selector, movement: MoveSpec, reason: MoveReason } => ["action.move", "receipt.move-after"],
    Destroy { objects: Selector } => ["action.destroy", "receipt.move-after"],
    Banish { objects: Selector } => ["action.banish", "receipt.move-after"],
    Discard { player: PlayerRef, mode: DiscardMode } => ["action.discard", "receipt.move-after"],
    Transfer { objects: Selector, destination_controller: PlayerRef } => ["action.transfer"],
    Damage { recipients: RecipientSet, amount: DamageAmount, flags: DamageFlags, source: ObjectView, batch: BatchPolicy } => ["action.damage"],
    Life { player: PlayerRef, change: NumericChange } => ["action.life"],
    Draw { player: PlayerRef, count: Expr<Int>, mode: StepwiseMode } => ["action.draw"],
    Look { viewer: PlayerRef, zone: ZoneRef, count: Expr<Nat>, mode: StepwiseMode } => ["action.look"],
    Search { player: PlayerRef, deck: PlayerRef, candidate: Binding<ObjectRef>, filter: Expr<Bool>, cardinality: Cardinality, groups: Option<SelectionConstraint>, destination: Option<SearchDestination>, continuation: SearchContinuation } => ["action.search", "receipt.move-after"],
    Reveal { objects: Selector } => ["action.reveal"],
    RevealUntil { deck: PlayerRef, candidate: Binding<ObjectRef>, predicate: Expr<Bool>, required: Expr<Nat> } => ["action.reveal-until"],
    Shuffle { deck: PlayerRef, scope: ShuffleScope, randomness: RandomnessPolicy } => ["action.shuffle"],
    Order { cards: Selector, destinations: Vec<Placement>, chooser: PlayerRef, visibility: Visibility } => ["action.order", "receipt.move-after"],
    Evolve { subject: ObjectOperand, form: EvolutionCardRef, mode: EvolutionMode, link: Binding<AssociationId> } => ["action.evolve"],
    Token { tokens: Vec<TokenSpec>, destination: TokenDestination, output: Binding<Selector>, initial_state: Option<TokenInitialState>, initial_posture: Posture, entry_effects: Vec<Effect> } => ["action.token"],
    Transform { objects: Selector, into: TokenAllocation, entry_effects: Vec<Effect> } => ["action.transform", "action.banish", "action.token", "receipt.move-after"],
    Counter { holder: Holder, kind: CounterKind, operation: CounterOperation, amount: Expr<Nat> } => ["action.counter"],
    FlipDown { objects: Selector } => ["action.flip-down"],
    FlipUp { objects: Selector, faces: Vec<VisibleFace> } => ["action.flip-up"],
    Posture { objects: Selector, posture: Posture } => ["action.posture"],
    Resource { player: PlayerRef, resource: Resource, operation: ResourceOperation, amount: Expr<Int> } => ["action.resource"],
    Modify { subjects: Selector, patch: CharacteristicPatch, expires: Duration, source: EffectReference } => ["action.modify", "effect.continuous"],
    Grant { subjects: Selector, abilities: Vec<GrantedAbility>, duration: Duration, provider: TextProvider } => ["action.grant"],
    RemoveAbility { subjects: Selector, scope: RemoveScope, duration: Duration, timestamp: TimestampBasis } => ["action.remove-ability"],
    Delay { trigger: TriggerSpec, body: Program, captures: Vec<Capture>, lifetime: DelayedLifetime, source: AbilitySource } => ["action.delay"],
    PlayCard { card: ObjectOperand, controller: PlayerRef, cost_override: Option<CostOverride>, authorization: EffectReference, parameters: PlaySpec } => ["action.play-card"],
    PlayAbility { ability: AbilityReference, controller: PlayerRef, cost_policy: AbilityCostPolicy, trigger_policy: AbilityTriggerPolicy } => ["action.play-ability"],
    MealRace { subject: ObjectOperand, operation: MealRaceOperation, amount: Expr<Nat>, resource_links: Vec<ObjectOperand> } => ["action.meal-race"],
    RideDrive { subject: ObjectOperand, operation: RideDriveOperation, usage: UsageReference } => ["action.ride-drive"],
    DriveCheck { player: PlayerRef, repetitions: Nat, source: AbilitySource } => ["action.drive-check"],
    Equip { holder: ObjectOperand, template: TokenTemplateId, count: Nat } => ["action.equip", "action.token"],
    Fusion { subject: ObjectOperand, materials: SelectionConstraint, cost_receipts: Vec<ReceiptId>, follow: MovedRef } => ["action.fusion", "receipt.move-after", "reference.follow-generation"],
    Random { population: Selector, count: Nat, rng: RandomnessPolicy, visibility: Visibility } => ["action.random"],
    Dice { operation: DiceOperation, result: Binding<Nat> } => ["action.dice"],
    Declare { actor: PlayerRef, declaration: Declaration } => ["action.declare"],
    ScheduleTurn { player: PlayerRef, operation: TurnOperation, occurrence: Binding<ScheduleToken> } => ["action.schedule-turn"],
    GameResult { players: PlayerSet, result: Outcome, source: AbilitySource } => ["action.game-result"],
    BoxPilot { subject: ObjectOperand, mode: BoxPilotMode } => ["action.box-pilot"]
});
record!(MoveSpec { to: ZoneRef, initial_posture: Option<Posture>, entry_effects: Vec<Effect>, position: Option<Position>, order: Option<Decision> });
tags!(MoveReason { Put, Return, Mill });
/// Exact quantity or an explicit per-card continue/stop decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepwiseMode {
    /// No optional stop decision.
    Exact,
    /// Repeated answers belong to this one instruction-local decision point.
    UpToStepwise {
        /// The instruction-local continue/stop decision.
        decision: Box<StepwiseDecision>,
    },
}
impl Walk for StepwiseMode {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        match self {
            Self::Exact => visitor.require("internal.StepwiseMode.Exact", path),
            Self::UpToStepwise { decision } => {
                visitor.require("internal.StepwiseMode.UpToStepwise", path);
                decision.walk(visitor, path);
            }
        }
    }
}
record!(StepwiseDecision {
    id: DecisionId,
    actor: PlayerRef,
    visibility: Visibility,
    timing: StepwiseTiming
});
tags!(StepwiseTiming {
    BeforeDraw,
    AfterLook
});
variants!(DiscardMode {
    Chosen { constraint: SelectionConstraint } => ["discard.chosen"],
    Random { population: Selector, count: Expr<Int> } => ["discard.random"],
    All { eligible: Selector } => ["discard.all"],
    DownTo { count: Nat } => ["discard.down-to"]
});
variants!(NumericChange { Add { amount: Expr<Int> } => ["numeric.add"], Set { amount: Expr<Int> } => ["numeric.set"] });
variants!(DamageRecipient { FromExpression { value: Box<Expr<DamageRecipient>> } => [], Bound { binding: Binding<DamageRecipient> } => [], Object { object: ObjectOperand } => [], Leader { player: PlayerRef } => [] });
variants!(DamageAmount { Uniform { amount: Expr<Int> } => [], Allocation { binding: Binding<Vec<DamageAllocation>> } => ["damage.allocation"] });
record!(DamageAllocation {
    recipient: DamageRecipient,
    amount: Nat
});
tags!(DamageFlags {
    Ability,
    Attack,
    Combat,
    AttackCombat
});
variants!(ShuffleScope { FullZone {} => [], Subset { cards: Selector } => ["shuffle.subset"] });
record!(Placement {
    cards: Selector,
    zone: ZoneRef,
    position: Position
});
record!(EvolutionCardRef {
    object: ObjectOperand,
    face: FaceId
});
tags!(EvolutionMode {
    Normal,
    Super,
    Effect
});
record!(TokenSpec { template: TokenTemplateId, name: Expr<CardName>, count: Expr<Nat> });
variants!(TokenAllocation {
    Unrestricted { templates: Vec<TokenTemplateId>, total: Expr<Nat>, counts: Binding<Vec<TokenCount>>, actor: PlayerRef, decision: DecisionId } => ["transform.token-allocation"],
    Single { template: TokenTemplateId } => [],
    EachExactlyOnce { templates: Vec<TokenTemplateId>, total: Expr<Nat>, counts: Binding<Vec<TokenCount>>, actor: PlayerRef, decision: DecisionId } => ["transform.token-allocation"]
});
variants!(CounterOperation { Add {} => [], Remove {} => [], Move { to: Holder } => ["counter.move"] });
record!(VisibleFace {
    object: ObjectOperand,
    face: FaceId
});
tags!(Resource {
    PpCurrent,
    PpMaximum,
    Ep,
    Sep
});
tags!(ResourceOperation {
    Add,
    Set,
    RecoverToMax,
    Pay
});
variants!(#[expect(variant_size_differences, reason = "The definition is already boxed; the remaining small discriminant stays inline.")] GrantedAbility { Definition { ability: Box<Ability> } => [], Keyword { keyword: KeywordId } => [] });
variants!(RemoveScope { AllAtTime {} => [], Matching { predicate: Box<AbilityPredicate> } => [] });
variants!(DelayedLifetime { Once {} => [], Until { duration: Box<Duration> } => [] });
variants!(CostOverride {
    WaiveOriginal {} => [], Alternate { cost: CostSpec } => [], SetBase { amount: Int } => [], AddDelta { amount: Expr<Int> } => ["effect.cost-delta"]
});
tags!(AbilityCostPolicy {
    Normal,
    WaiveOriginal
});
tags!(AbilityTriggerPolicy {
    Normal,
    IgnoreAsAuthorized
});
tags!(MealRaceOperation { Meal, Race });
variants!(RideDriveOperation { AttachDrivePoint { resource: ObjectOperand } => [], GainDrive {} => [] });
variants!(#[derive(Copy)] DiceOperation { Roll {} => [], Reroll { permission: RuleId, roll: Binding<RollId> } => ["dice.reroll"] });
variants!(#[derive(Copy)] Declaration {
    NonnegativeInteger { binding: Binding<Nat> } => [],
    BoundedInteger { minimum: Int, maximum: Int, binding: Binding<Int> } => [],
    CardNames { snapshot: CardSnapshot, binding: Binding<CardName> } => []
});
variants!(#[derive(Copy)] TurnOperation { InsertBefore { anchor: ScheduleAnchor } => [], SkipNext {} => [] });
tags!(Outcome { Win, Lose });
variants!(BoxPilotMode {
    Boxed { duration: Duration, cutoff: TimestampBasis } => [],
    Pilot { duration: Duration, attack: Int, life: Int } => []
});

variants!(InstallationSubjects { Objects { objects: Selector, affected: Binding<Selector> } => [], Players { players: PlayerSet, affected: Binding<PlayerSet> } => [] });
tags!(InstallationTime {
    InstructionStart,
    AtPlay
});
variants!(SearchDestination {
    Single { movement: MoveSpec } => [],
    Partition { destinations: Vec<SearchPartition> } => [],
    OptionalSelfReplacement { predicate: Expr<Bool>, movement: MoveSpec } => []
});
record!(SearchPartition {
    zone: ZoneRef,
    cardinality: Cardinality
});
variants!(DestinationAllocation { Batch { domain: Vec<ZoneRef> } => [], PerItem { domain: Vec<ZoneRef> } => [] });

variants!(TokenInitialState { StackCounters { amount: Expr<Nat> } => ["action.counter"] });

variants!(TokenDestination { Single { zone: Expr<ZoneRef> } => [], Allocated { allocation: DestinationAllocation } => [] });
record!(TokenCount {
    template: TokenTemplateId,
    count: Nat
});
record!(#[derive(Copy)] SearchContinuation { found: Binding<SearchFound>, revealed: Binding<SearchRevealed> });
/// Search-only continuation binding; not an object selector or a moved result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchFound {}
/// Search-only public proof binding; not an object selector or a moved result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchRevealed {}

variants!(RecipientSet {
    Objects { objects: Selector } => ["recipient.objects"],
    Leaders { players: PlayerSet } => ["recipient.leaders"],
    Union { sets: Vec<RecipientSet> } => ["recipient.union"],
    Explicit { recipients: Vec<DamageRecipient> } => ["recipient.explicit"],
    Bound { recipients: Binding<Vec<DamageRecipient>> } => []
});
