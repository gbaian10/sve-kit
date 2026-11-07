//! A hand card's own banishment preserves its subject before the generation changes.
use crate::ir::{
    BanishEvent, Binding, Bool, EventPattern, EventSnapshotPolicy, Expr, ObjectFrozen, PlayerRef,
    Predicate, TriggerAggregation, TriggerCondition, TriggerSpec, ZoneFrozen, ZoneKind,
    ZoneOperation,
};

pub(crate) fn predicate(event: Binding<BanishEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            Expr::Compute(Box::new(Predicate::ObjectEquals {
                left: Expr::Frozen(Box::new(ObjectFrozen::BanishCardBefore { event })),
                right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
            })),
            Expr::Compute(Box::new(Predicate::ZoneEquals {
                left: Expr::Frozen(Box::new(ZoneFrozen::BanishFrom { event })),
                right: Expr::Compute(Box::new(ZoneOperation::OfPlayer {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Hand,
                })),
            })),
        ],
    }))
}

pub(crate) fn matches(event: Binding<BanishEvent>, condition: &Expr<Bool>) -> bool {
    *condition == predicate(event)
}

pub(crate) fn self_event(trigger: &TriggerSpec) -> Option<Binding<BanishEvent>> {
    let TriggerCondition::Event {
        pattern: EventPattern::Banish { binding, predicate },
    } = &trigger.on
    else {
        return None;
    };
    (matches(*binding, predicate)
        && trigger.active_zones == [ZoneKind::Hand]
        && trigger.view == EventSnapshotPolicy::BeforeExit
        && trigger.aggregation == (TriggerAggregation::PerFact {}))
    .then_some(*binding)
}
