//! Closed public history queries retain their existing counter and object-state authorities.
use crate::ir::StatChangeEvent;
use crate::ir::events::change::StatFilter;
use crate::ir::{
    BanishEvent, Binding, Bool, BoolRead, CardKind, CardName, Cardinality, CharacteristicView,
    DamageEvent, DamageRecipient, EventContext, EventPattern, EventSnapshotPolicy, Expr,
    HistoryCutoff, HistoryQuery, HistoryRange, HistoryWindow, InclusionPolicy, KindRead, NamesRead,
    Nat, ObjectFrozen, ObjectOperand, ObjectRef, ObjectView, PlayerRef, PlayerRelation, PlayerSet,
    Predicate, RecipientFields, SelectionView, Selector, TextQuantifier, ZoneFrozen, ZoneKind,
    ZoneOperation,
};

const fn current_turn(pattern: EventPattern) -> HistoryQuery {
    HistoryQuery {
        pattern,
        window: HistoryWindow {
            range: HistoryRange::CurrentTurn {},
            cutoff: HistoryCutoff::CurrentPoint {},
            relative_player: None,
        },
        include_execution: InclusionPolicy::IncludeAll {},
    }
}

pub(crate) fn stat_gain_query(
    event: Binding<StatChangeEvent>,
    filter: &StatFilter,
) -> HistoryQuery {
    current_turn(EventPattern::StatChange {
        binding: event,
        predicate: filter.predicate(event),
    })
}

pub(crate) fn stat_gain_plan(query: &HistoryQuery) -> Option<StatFilter> {
    let EventPattern::StatChange { binding, predicate } = &query.pattern else {
        return None;
    };
    let filter = StatFilter::parse(*binding, predicate)?;
    (filter.itself
        && filter.up == Some(true)
        && matches!(filter.properties.as_slice(), [_] | [_, _])
        && !matches!(filter.properties.as_slice(), [first, second] if first == second)
        && *query == stat_gain_query(*binding, &filter))
    .then_some(filter)
}

pub(crate) fn banished_query(event: Binding<BanishEvent>, name: CardName) -> HistoryQuery {
    current_turn(EventPattern::Banish {
        binding: event,
        predicate: Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::ZoneEquals {
                    left: Expr::Frozen(Box::new(ZoneFrozen::BanishFrom { event })),
                    right: Expr::Compute(Box::new(ZoneOperation::OfPlayer {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex,
                    })),
                })),
                Expr::Compute(Box::new(Predicate::NamesIntersect {
                    left: Expr::Live(Box::new(NamesRead::Names {
                        object: ObjectView::EventSnapshot {
                            object: Box::new(ObjectOperand::FromExpression {
                                value: Box::new(Expr::Frozen(Box::new(
                                    ObjectFrozen::BanishCardBefore { event },
                                ))),
                            }),
                            event: EventContext::Banish { event },
                            policy: EventSnapshotPolicy::BeforeExit,
                        },
                    })),
                    right: Expr::Literal(vec![name]),
                })),
            ],
        })),
    })
}

pub(crate) fn banished_plan(query: &HistoryQuery) -> Option<&CardName> {
    let EventPattern::Banish {
        binding,
        predicate: Expr::Compute(operation),
    } = &query.pattern
    else {
        return None;
    };
    let Predicate::And { predicates } = operation.as_ref() else {
        return None;
    };
    let Expr::Compute(last) = predicates.last()? else {
        return None;
    };
    let Predicate::NamesIntersect {
        right: Expr::Literal(names),
        ..
    } = last.as_ref()
    else {
        return None;
    };
    let [name] = names.as_slice() else {
        return None;
    };
    (!name.0.is_empty() && *query == banished_query(*binding, name.clone())).then_some(name)
}

pub(crate) fn banished_condition(condition: &Expr<Bool>) -> Option<&CardName> {
    let Expr::Live(read) = condition else {
        return None;
    };
    let BoolRead::HistoryExists { query } = read.as_ref() else {
        return None;
    };
    banished_plan(query)
}

fn damaged_query(event: Binding<DamageEvent>, candidate: Binding<ObjectRef>) -> HistoryQuery {
    current_turn(EventPattern::Damage {
        binding: event,
        predicate: Expr::Compute(Box::new(Predicate::RecipientIsFollower {
            recipient: DamageRecipient::FromExpression {
                value: Box::new(Expr::Frozen(Box::new(RecipientFields::DamageRecipient {
                    event,
                }))),
            },
            objects: Selector::Objects {
                objects: vec![ObjectOperand::Bound { binding: candidate }],
            },
        })),
    })
}

pub(crate) fn damaged_selector(
    candidate: Binding<ObjectRef>,
    event: Binding<DamageEvent>,
) -> Selector {
    Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(0),
                maximum: None,
            },
            text_quantifier: TextQuantifier::Implicit,
        },
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding: candidate,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::KindEquals {
                    left: Expr::Live(Box::new(KindRead::Kind {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::Bound { binding: candidate }),
                        },
                        view: CharacteristicView::Effective,
                    })),
                    right: Expr::Literal(CardKind::Follower),
                })),
                Expr::Live(Box::new(BoolRead::HistoryExists {
                    query: damaged_query(event, candidate),
                })),
            ],
        }))),
    }
}

pub(crate) fn damaged_plan(
    selector: &Selector,
) -> Option<(Binding<ObjectRef>, Binding<DamageEvent>)> {
    let Selector::Zone {
        binding, predicate, ..
    } = selector
    else {
        return None;
    };
    let Expr::Compute(operation) = predicate.as_ref() else {
        return None;
    };
    let Predicate::And { predicates } = operation.as_ref() else {
        return None;
    };
    let Expr::Live(read) = predicates.last()? else {
        return None;
    };
    let BoolRead::HistoryExists { query } = read.as_ref() else {
        return None;
    };
    let EventPattern::Damage { binding: event, .. } = &query.pattern else {
        return None;
    };
    (*selector == damaged_selector(*binding, *event)).then_some((*binding, *event))
}
