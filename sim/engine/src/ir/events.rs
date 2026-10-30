pub(crate) mod entry;
pub(crate) mod movement;
pub(crate) mod played;
pub(crate) mod timing;
use super::{
    AttackEvent, BanishEvent, BatchPolicy, Binding, Bool, CounterChangeEvent, DamageEvent,
    DestroyEvent, DiceEvent, DiscardEvent, DrawEvent, DriveCheckedEvent, DriveGainedEvent,
    DriveTriggerEvent, EarthRitePaidComponent, EnterExEvent, EnterFieldEvent, EquipEvent,
    EvolveEvent, ExecutionRef, Expr, FusedEvent, HistoryAnchor, LeaveFieldEvent, LifeChangeEvent,
    MoveEvent, Nat, PaymentEvent, PhaseEvent, PlayEvent, PlayerRef, PostureEvent, RaceEvent,
    ResolveEvent, SnapshotId, Stat, StatChangeEvent, TargetedEvent, UbActivatedEvent, Visitor,
    Walk, ZoneKind,
};
use core::num::NonZeroU64;

tags!(EventSnapshotPolicy {
    BeforeExit,
    AtReveal,
    AfterEntry
});

/// Strictly positive event damage; zero is not a damage fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PositiveInt(pub NonZeroU64);
impl Walk for PositiveInt {
    fn walk(&self, _visitor: &mut Visitor, _path: &str) {}
}
tags!(DieFace {
    One,
    Two,
    Three,
    Four,
    Five,
    Six
});
tags!(TriggerIcon {
    Critical,
    Draw,
    Stand,
    Heal,
    Front
});
tags!(Completion { Proper, Other });

tags!(MoveMethod {
    Put,
    Return,
    Mill,
    Draw,
    Discard,
    Destroy,
    Banish,
    Play
});
tags!(EntryMethod {
    Play,
    AbilityPut,
    RulePut
});
variants!(#[derive(Copy)] EntryOrigin { Zone { zone: ZoneKind } => [], Other {} => [] });

tags!(StatOperation { Add, Set });
tags!(StatChangeReason {
    Instruction,
    SuperEvolutionRule,
    Damage,
    Recovery,
    Payment
});

/// Absence by rule is a value, never an unknown field fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Info<T> {
    /// Information exists.
    Present(T),
    /// The game rules give this object no such information.
    AbsentByRules,
}
impl<T: Walk> Walk for Info<T> {
    fn walk(&self, visitor: &mut Visitor, path: &str) {
        match self {
            Self::Present(value) => {
                visitor.require("shape.Info.Present", path);
                value.walk(visitor, path);
            }
            Self::AbsentByRules => visitor.require("shape.Info.AbsentByRules", path),
        }
    }
}

variants!(EventPattern {
    EarthRitePayment { event: Binding<PaymentEvent>, component: Binding<EarthRitePaidComponent>, predicate: Expr<Bool> } => ["event.payment.earth-rite"],
    Play { binding: Binding<PlayEvent>, predicate: Expr<Bool> } => ["event.match.Play"],
    Targeted { binding: Binding<TargetedEvent>, predicate: Expr<Bool> } => ["event.match.Targeted"],
    Resolve { binding: Binding<ResolveEvent>, predicate: Expr<Bool> } => ["event.match.Resolve"],
    Payment { binding: Binding<PaymentEvent>, predicate: Expr<Bool> } => ["event.match.Payment"],
    Move { binding: Binding<MoveEvent>, predicate: Expr<Bool> } => ["event.match.Move"],
    EnterField { binding: Binding<EnterFieldEvent>, predicate: Expr<Bool> } => ["event.match.EnterField"],
    LeaveField { binding: Binding<LeaveFieldEvent>, predicate: Expr<Bool> } => ["event.match.LeaveField"],
    EnterEx { binding: Binding<EnterExEvent>, predicate: Expr<Bool> } => ["event.match.EnterEx"],
    Draw { binding: Binding<DrawEvent>, predicate: Expr<Bool> } => ["event.match.Draw"],
    Discard { binding: Binding<DiscardEvent>, predicate: Expr<Bool> } => ["event.match.Discard"],
    Destroy { binding: Binding<DestroyEvent>, predicate: Expr<Bool> } => ["event.match.Destroy"],
    Banish { binding: Binding<BanishEvent>, predicate: Expr<Bool> } => ["event.match.Banish"],
    Damage { binding: Binding<DamageEvent>, predicate: Expr<Bool> } => ["event.match.Damage"],
    StatChange { binding: Binding<StatChangeEvent>, predicate: Expr<Bool> } => ["event.match.StatChange"],
    LifeChange { binding: Binding<LifeChangeEvent>, predicate: Expr<Bool> } => ["event.match.LifeChange"],
    CounterChange { binding: Binding<CounterChangeEvent>, predicate: Expr<Bool> } => ["event.match.CounterChange"],
    Posture { binding: Binding<PostureEvent>, predicate: Expr<Bool> } => ["event.match.Posture"],
    Evolve { binding: Binding<EvolveEvent>, predicate: Expr<Bool> } => ["event.match.Evolve"],
    Attack { binding: Binding<AttackEvent>, predicate: Expr<Bool> } => ["event.match.Attack"],
    Phase { binding: Binding<PhaseEvent>, predicate: Expr<Bool> } => ["event.match.Phase"],
    Race { binding: Binding<RaceEvent>, predicate: Expr<Bool> } => ["event.match.Race"],
    DriveGained { binding: Binding<DriveGainedEvent>, predicate: Expr<Bool> } => ["event.match.DriveGained"],
    DriveTrigger { binding: Binding<DriveTriggerEvent>, predicate: Expr<Bool> } => ["event.match.DriveTrigger"],
    UbActivated { binding: Binding<UbActivatedEvent>, predicate: Expr<Bool> } => ["event.match.UbActivated"],
    Fused { binding: Binding<FusedEvent>, predicate: Expr<Bool> } => ["event.match.Fused"],
    Equip { binding: Binding<EquipEvent>, predicate: Expr<Bool> } => ["event.match.Equip"],
    Dice { binding: Binding<DiceEvent>, predicate: Expr<Bool> } => ["event.match.Dice"],
    DriveChecked { binding: Binding<DriveCheckedEvent>, predicate: Expr<Bool> } => ["event.match.DriveChecked"],
    AnyOf { patterns: Vec<EventPattern> } => ["event.any-of"]
});
record!(TriggerSpec { on: TriggerCondition, active_zones: Vec<ZoneKind>, view: EventSnapshotPolicy, limit: Option<LimitSpec>, aggregation: TriggerAggregation });
variants!(TriggerCondition { Event { pattern: EventPattern } => ["trigger.event"], State { predicate: Expr<Bool>, latch: Binding<Bool> } => ["trigger.state"] });
variants!(#[derive(Copy)] TriggerAggregation { PerFact {} => ["trigger.per-fact"], PerBatch { key: BatchPolicy } => ["trigger.per-batch"] });
record!(
    #[derive(Copy)]
    LimitSpec {
        maximum: Nat,
        window: LimitWindow,
        scope: LimitScope,
        consume_at: ConsumptionPoint
    }
);
tags!(LimitWindow {
    Turn,
    Game,
    ObjectGeneration
});
tags!(LimitScope {
    AbilityInstance,
    RulePermission
});
tags!(ConsumptionPoint {
    PendingAdmission,
    PlayCommitted,
    RuleDefined
});

record!(HistoryQuery {
    pattern: EventPattern,
    window: HistoryWindow,
    include_execution: InclusionPolicy
});
record!(HistoryWindow { range: HistoryRange, cutoff: HistoryCutoff, relative_player: Option<PlayerRef> });
variants!(#[derive(Copy)] HistoryRange { CurrentTurn {} => [], Game {} => [], Since { anchor: HistoryAnchor } => [] });
variants!(#[derive(Copy)] HistoryCutoff { Capture { snapshot: Binding<SnapshotId> } => [], CurrentPoint {} => [] });
variants!(#[derive(Copy)] InclusionPolicy { IncludeAll {} => [], ExcludeCurrentExecution { execution: ExecutionRef } => [] });
tags!(HistoryCount { Facts, Subjects });

variants!(NumericHistoryQuery {
    EarthRiteRemovedStack { event: Binding<PaymentEvent>, component: Binding<EarthRitePaidComponent>, predicate: Expr<Bool>, window: HistoryWindow, include_execution: InclusionPolicy } => ["history.sum.earth-rite-stack", "event.payment.earth-rite"],
    Damage { event: Binding<DamageEvent>, predicate: Expr<Bool>, window: HistoryWindow, include_execution: InclusionPolicy } => ["history.sum.damage"],
    LifeDelta { event: Binding<LifeChangeEvent>, predicate: Expr<Bool>, window: HistoryWindow, include_execution: InclusionPolicy } => ["history.sum.life-delta"],
    CounterDelta { event: Binding<CounterChangeEvent>, predicate: Expr<Bool>, window: HistoryWindow, include_execution: InclusionPolicy } => ["history.sum.counter-delta"],
    StatDelta { event: Binding<StatChangeEvent>, property: Stat, predicate: Expr<Bool>, window: HistoryWindow, include_execution: InclusionPolicy } => ["history.sum.stat-delta"]
});

impl TriggerSpec {
    pub(crate) fn lastword(event: Binding<LeaveFieldEvent>) -> Self {
        Self {
            on: TriggerCondition::Event {
                pattern: EventPattern::LeaveField {
                    binding: event,
                    predicate: Expr::Compute(Box::new(super::Predicate::And {
                        predicates: vec![
                            Expr::Compute(Box::new(super::Predicate::ObjectEquals {
                                left: Expr::Frozen(Box::new(
                                    super::ObjectFrozen::LeaveFieldObjectBefore { event },
                                )),
                                right: Expr::Frozen(Box::new(super::ObjectFrozen::SourceHolder {})),
                            })),
                            Expr::Compute(Box::new(super::Predicate::ZoneEquals {
                                left: Expr::Frozen(Box::new(super::ZoneFrozen::LeaveFieldTo {
                                    event,
                                })),
                                right: Expr::Compute(Box::new(super::ZoneOperation::OfPlayer {
                                    player: PlayerRef::Owner {},
                                    kind: ZoneKind::Cemetery,
                                })),
                            })),
                        ],
                    })),
                },
            },
            active_zones: vec![ZoneKind::Field],
            view: EventSnapshotPolicy::BeforeExit,
            limit: None,
            aggregation: TriggerAggregation::PerFact {},
        }
    }
}

impl LimitSpec {
    pub(crate) const fn activated_per_turn(maximum: Nat) -> Self {
        Self {
            maximum,
            window: LimitWindow::Turn,
            scope: LimitScope::AbilityInstance,
            consume_at: ConsumptionPoint::PlayCommitted,
        }
    }

    pub(crate) fn is_activated(self) -> bool {
        matches!(self.maximum, Nat(1 | 2)) && self == Self::activated_per_turn(self.maximum)
    }

    pub(crate) const fn once_per_turn() -> Self {
        Self {
            maximum: Nat(1),
            window: LimitWindow::Turn,
            scope: LimitScope::AbilityInstance,
            consume_at: ConsumptionPoint::PendingAdmission,
        }
    }
}
