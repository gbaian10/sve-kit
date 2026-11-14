//! A hand card's own banishment preserves its subject before the generation changes.
use crate::ir::{
    BanishEvent, Binding, Bool, EventPattern, EventSnapshotPolicy, Expr, ObjectFrozen,
    PlayerFrozen, PlayerRef, Predicate, TriggerAggregation, TriggerCondition, TriggerSpec,
    ZoneFrozen, ZoneKind, ZoneOperation,
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

/// Whose banish zone receives the card, and whether only during the controller's turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ZonePlan {
    pub(crate) own: bool,
    pub(crate) own_turn: bool,
}

/// 「(自分のターン中、)自分の消滅領域にカードが置かれたとき」: every card arriving in that zone.
pub(crate) fn zone_predicate(event: Binding<BanishEvent>, own: bool, own_turn: bool) -> Expr<Bool> {
    let mut predicates = vec![Expr::Compute(Box::new(Predicate::PlayerEquals {
        left: Expr::Frozen(Box::new(PlayerFrozen::BanishOwner { event })),
        right: Expr::Literal(if own {
            PlayerRef::Controller {}
        } else {
            super::timing::opponent()
        }),
    }))];
    if own_turn {
        predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
            left: Expr::Literal(PlayerRef::Active {}),
            right: Expr::Literal(PlayerRef::Controller {}),
        })));
    }
    Expr::Compute(Box::new(Predicate::And { predicates }))
}

/// Either closed banish header: a hand card's own banishment or a banish-zone observer.
pub(crate) fn admitted(event: Binding<BanishEvent>, condition: &Expr<Bool>) -> bool {
    matches(event, condition)
        || [(true, false), (true, true), (false, false), (false, true)]
            .into_iter()
            .any(|(own, own_turn)| *condition == zone_predicate(event, own, own_turn))
}

/// A printed field observer of its controller's or the opponent's banish zone.
pub(crate) fn zone_plan(trigger: &TriggerSpec) -> Option<ZonePlan> {
    let TriggerCondition::Event {
        pattern: EventPattern::Banish { binding, predicate },
    } = &trigger.on
    else {
        return None;
    };
    if trigger.active_zones != [ZoneKind::Field]
        || trigger.view != EventSnapshotPolicy::BeforeExit
        || trigger.aggregation != (TriggerAggregation::PerFact {})
    {
        return None;
    }
    [(true, false), (true, true), (false, false), (false, true)]
        .into_iter()
        .map(|(own, own_turn)| ZonePlan { own, own_turn })
        .find(|plan| *predicate == zone_predicate(*binding, plan.own, plan.own_turn))
}
