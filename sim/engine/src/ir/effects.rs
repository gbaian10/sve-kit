use super::{
    AbilityKind, AbilityOrigin, AbilitySource, Binding, Bool, CaptureRef, CardKind, CardName,
    Cardinality, CausePattern, CharacteristicView, CounterKind, DamageRecipient, Edge,
    EffectDefinitionId, EffectReference, EventPattern, ExecutionRef, Expr, GrantedAbility, Holder,
    Int, KeywordId, MoveDestination, Nat, NumericChange, ObjectOperand, ObjectRef, Phase,
    PlayerRef, PlayerSet, Position, PositiveInt, Posture, RecipientSet, RuleId, RuleProcedure,
    Selector, Stat, TimeAnchor, TitleId, TraitId, Visibility, ZoneKind,
};

variants!(Effect {
    Continuous { id: EffectDefinitionId, source: AbilitySource, target: EffectTarget, condition: Option<Expr<Bool>>, patch: CharacteristicPatch, layer: ContinuousLayer, timestamp: TimestampBasis, active_zones: Vec<ZoneKind>, duration: Duration } => ["effect.continuous"],
    Replacement { id: EffectDefinitionId, source: AbilitySource, pattern: ProposedEventPattern, condition: Expr<Bool>, rewrite: EventRewrite, scope: ReplacementScope, affected_player: PlayerRef, once_per_event: OncePerEvent, usage: Option<UseLimit>, prospective_chars: Option<CharacteristicView>, duration: Duration } => ["effect.replacement"],
    Permission { subject: PermissionSubject, action: ActionPattern, polarity: Polarity, condition: Expr<Bool>, duration: Duration, rule: RuleId } => ["effect.permission"],
    RuleModifier { modifier: RuleModifier, condition: Expr<Bool>, duration: Duration, source: AbilitySource } => ["effect.rule-modifier"],
    DeckRule { scope: DeckRuleScope, basis: DeckBasis, constraint: DeckConstraint, setup_steps: Vec<RuleProcedure>, visibility: Visibility, rule: RuleId } => ["effect.deck-rule"],
    CostModifier { subjects: Selector, change: CostChange, duration: Duration, usage: Option<UsageScope> } => ["effect.cost-modifier"]
});
variants!(EffectTarget { Dynamic { selector: Selector } => [], Captured { objects: CaptureRef<Selector> } => [] });
tags!(ContinuousLayer {
    Ability,
    NonNumeric,
    Numeric
});
variants!(TimestampBasis { ZoneEntry { object: ObjectOperand } => [], ProperPlay { execution: ExecutionRef } => [] });
variants!(CharacteristicPatch {
    GrantAbility { abilities: Vec<GrantedAbility> } => [],
    SuppressAbility { predicate: Box<AbilityPredicate> } => [],
    SetKind { kind: CardKind } => [],
    AddName { name: CardName } => [],
    AddTrait { trait_id: TraitId } => [],
    RemoveTrait { trait_id: TraitId } => [],
    Numeric { stat: Stat, change: NumericChange } => [],
    Combined { patches: Vec<CharacteristicPatch> } => []
});
record!(Duration { end: DurationEnd, anchor: TimeAnchor, usage: Option<UseLimit>, intervals: Vec<Interval> });
variants!(DurationEnd {
    CurrentTurnEnd {} => ["duration.turn"],
    NextTurnEnd { player: PlayerRef } => ["duration.next-turn-end"],
    EndResolution {} => ["duration.resolution"],
    While { predicate: Expr<Bool> } => ["duration.while"],
    SourceZoneExit { source: ObjectOperand, zone: ZoneKind } => ["duration.source-zone-exit"],
    NoTemporalExpiry {} => ["duration.no-expiry"],
    NextPhaseEdge { player: PlayerRef, phase: Phase, edge: Edge } => ["duration.next-phase-edge"],
    NextTurnEdge { player: PlayerRef, edge: Edge } => ["duration.next-turn-edge"]
});
variants!(Interval { DuringNextPhase { player: PlayerRef, phase: Phase } => ["duration.next-phase-interval"], DuringNextTurn { player: PlayerRef } => ["duration.next-turn-interval"] });
record!(UseLimit {
    count: Nat,
    scope: UsageScope
});
variants!(UsageScope { PerObject { object: ObjectOperand } => [], SharedPlayer { player: PlayerRef, effect: EffectReference } => ["usage.shared-player"] });
variants!(CostChange { Delta { amount: Expr<Int> } => ["effect.cost-delta"], Set { amount: Expr<Int> } => ["effect.cost-set"] });
tags!(ReplacementScope { SelfOnly, General });
variants!(ProposedEventPattern {
    LeavesZone { objects: Selector, from: ZoneKind } => [],
    Damage { recipient: Option<RecipientPredicate>, source: Option<SourcePredicate>, kind: DamageKind } => [],
    Move { objects: Selector, to: ZoneKind } => [],
    Instruction { pattern: InstructionPattern } => []
});
tags!(DamageKind {
    Any,
    Combat,
    Ability
});
variants!(EventRewrite {
    RedirectMove { to: MoveDestination, position: Option<Position> } => ["replacement.move.redirect"],
    PreventDamage { recipient: Option<RecipientPredicate>, source: Option<SourcePredicate>, kind: DamageKind } => ["replacement.damage.prevent"],
    AddDamage { amount: Expr<Int> } => ["replacement.damage.add"],
    MultiplyDamage { numerator: Nat, denominator: PositiveInt, rounding: Rounding } => ["replacement.damage.multiply"],
    CapDamage { threshold: Expr<Nat>, replacement: Expr<Nat> } => ["replacement.damage.cap"],
    EntryPosture { posture: Posture } => ["replacement.entry-posture"],
    EvolutionCost { change: CostChange } => ["replacement.evolution-cost"],
    ReplaceInstruction { rewrite: InstructionRewrite } => ["replacement.instruction"]
});
tags!(Rounding { Exact, Floor, Ceil });
variants!(InstructionPattern {
    Damage { recipients: RecipientSet } => [], ReturnToHand { objects: Selector } => [], Banish { objects: Selector } => []
});
variants!(InstructionRewrite {
    DamageToDestroy { objects: Selector } => ["action.destroy"],
    DamageToBanish { objects: Selector } => ["action.banish"],
    ReturnToDestroy { objects: Selector } => ["action.destroy"],
    SingleToAllBanish { objects: Selector } => ["action.banish"]
});
variants!(PermissionSubject { Players { players: PlayerSet } => [], Objects { objects: Selector } => [] });
tags!(Polarity {
    Allow,
    Forbid,
    Require
});
variants!(ActionPattern {
    PlayCard { objects: Selector } => [], PlayAbility { predicate: AbilityPredicate } => [],
    Attack { attacker: Selector, target: Option<RecipientSet> } => [],
    ChooseTarget { objects: Selector } => [], Draw { player: PlayerRef } => [],
    Destroy { objects: Selector, cause: CausePattern } => [], Banish { objects: Selector, cause: CausePattern } => [],
    Stand { objects: Selector, scope: StandScope } => [], IncreasePpMaximum { player: PlayerRef } => [],
    IgnoreWard { attacker: Selector } => [], Reroll { player: PlayerRef, times: Nat } => [],
    ChooseMode { cardinality: Cardinality } => [], Win { players: PlayerSet } => [],
    Lose { players: PlayerSet } => [], TriggerAdmission { predicate: AbilityPredicate } => []
});
variants!(StandScope { Any {} => [], RuleStart { player: PlayerRef } => [] });
record!(AbilityPredicate { kind: Option<AbilityKind>, keyword: Option<KeywordId>, except: Vec<KeywordId>, holder: Option<Selector>, origin: Option<AbilityOrigin> });
variants!(RuleModifier {
    SpellChainPopulation { player: PlayerRef, population: Selector } => ["rule.spell-chain-population"],
    CounterCap { holder: Holder, kind: CounterKind, cap: Nat } => ["rule.counter-cap"],
    EvolutionPlayLimit { player: PlayerRef } => ["rule.evolution-play-limit"],
    BattleDamageBasis { objects: Selector } => ["rule.battle-damage-basis"],
    TriggerMultiplicity { pattern: EventPattern, controller: PlayerRef, add: Nat } => ["rule.trigger-multiplicity"]
});
tags!(DeckRuleScope { Deck, Setup });
tags!(DeckBasis { Class, Title });
variants!(DeckConstraint {
    SameNameLimit { name: CardName, maximum: Nat } => [],
    StartAmuletDistinctNameLimit { maximum: Nat } => [],
    Title { title: TitleId } => []
});

tags!(OncePerEvent { Enforced });
record!(RecipientPredicate { binding: Binding<DamageRecipient>, condition: Expr<Bool> });
record!(SourcePredicate { binding: Binding<ObjectRef>, condition: Expr<Bool> });
