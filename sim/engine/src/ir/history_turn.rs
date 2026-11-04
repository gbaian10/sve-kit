//! Current-turn queries share the closed public subject filters used by event headers.

use super::events::discard::DiscardFilter;
use super::events::subject::{SubjectFields, SubjectFilter};
use super::{
    Binding, Bool, DiscardEvent, EventPattern, ExecutionRef, Expr, HistoryCutoff, HistoryQuery,
    HistoryRange, HistoryWindow, InclusionPolicy, PlayEvent, PlayerFrozen, PlayerRef, Predicate,
    UbActivatedEvent,
};

pub(crate) fn played_query(event: Binding<PlayEvent>, player: Option<PlayerRef>) -> HistoryQuery {
    HistoryQuery {
        pattern: EventPattern::Play {
            binding: event,
            predicate: player.map_or(Expr::Literal(Bool(true)), |player| {
                Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Frozen(Box::new(PlayerFrozen::PlayController { event })),
                    right: Expr::Literal(player),
                }))
            }),
        },
        window: HistoryWindow {
            range: HistoryRange::CurrentTurn {},
            cutoff: HistoryCutoff::CurrentPoint {},
            relative_player: None,
        },
        include_execution: InclusionPolicy::IncludeAll {},
    }
}

pub(crate) enum PlayedBy {
    Any,
    Player(PlayerRef),
}

pub(crate) fn discarded_query(event: Binding<DiscardEvent>) -> HistoryQuery {
    HistoryQuery {
        pattern: EventPattern::Discard {
            binding: event,
            predicate: DiscardFilter {
                own: Some(true),
                itself: false,
                own_turn: false,
                by: None,
            }
            .predicate(event),
        },
        window: HistoryWindow {
            range: HistoryRange::CurrentTurn {},
            cutoff: HistoryCutoff::CurrentPoint {},
            relative_player: None,
        },
        include_execution: InclusionPolicy::IncludeAll {},
    }
}

pub(crate) fn discarded_plan(query: &HistoryQuery) -> bool {
    matches!(&query.pattern, EventPattern::Discard { binding, .. }
        if *query == discarded_query(*binding))
}

pub(crate) fn played_by(query: &HistoryQuery) -> Option<PlayedBy> {
    let EventPattern::Play { binding, predicate } = &query.pattern else {
        return None;
    };
    let player = match predicate {
        Expr::Literal(Bool(true)) => None,
        Expr::Compute(operation) => {
            let Predicate::PlayerEquals {
                left,
                right: Expr::Literal(player),
            } = operation.as_ref()
            else {
                return None;
            };
            if *left != Expr::Frozen(Box::new(PlayerFrozen::PlayController { event: *binding })) {
                return None;
            }
            Some(player.clone())
        }
        Expr::Literal(_) | Expr::Frozen(_) | Expr::Live(_) | Expr::Captured(_) | Expr::Bound(_) => {
            return None;
        }
    };
    (query == &played_query(*binding, player.clone()))
        .then_some(player.map_or(PlayedBy::Any, PlayedBy::Player))
}

pub(crate) fn ub_query(
    event: Binding<UbActivatedEvent>,
    player: Option<PlayerRef>,
    exclude_current: bool,
) -> HistoryQuery {
    HistoryQuery {
        pattern: EventPattern::UbActivated {
            binding: event,
            predicate: player.map_or(Expr::Literal(Bool(true)), |player| {
                Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Frozen(Box::new(PlayerFrozen::UbActivatedController { event })),
                    right: Expr::Literal(player),
                }))
            }),
        },
        window: HistoryWindow {
            range: HistoryRange::CurrentTurn {},
            cutoff: HistoryCutoff::CurrentPoint {},
            relative_player: None,
        },
        include_execution: if exclude_current {
            InclusionPolicy::ExcludeCurrentExecution {
                execution: ExecutionRef::Current {},
            }
        } else {
            InclusionPolicy::IncludeAll {}
        },
    }
}

pub(crate) fn ub_plan(query: &HistoryQuery) -> Option<(Option<PlayerRef>, bool)> {
    let EventPattern::UbActivated { binding, predicate } = &query.pattern else {
        return None;
    };
    let player = match predicate {
        Expr::Literal(Bool(true)) => None,
        Expr::Compute(operation) => {
            let Predicate::PlayerEquals {
                left,
                right: Expr::Literal(player),
            } = operation.as_ref()
            else {
                return None;
            };
            if *left
                != Expr::Frozen(Box::new(PlayerFrozen::UbActivatedController {
                    event: *binding,
                }))
            {
                return None;
            }
            Some(player.clone())
        }
        Expr::Literal(_) | Expr::Frozen(_) | Expr::Live(_) | Expr::Captured(_) | Expr::Bound(_) => {
            return None;
        }
    };
    let exclude_current = matches!(
        query.include_execution,
        InclusionPolicy::ExcludeCurrentExecution { .. }
    );
    (query == &ub_query(*binding, player.clone(), exclude_current))
        .then_some((player, exclude_current))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubjectHistoryKind {
    Attack,
    Evolve,
    Destroy,
    Leave,
    Cemetery,
    Hand,
}

pub(crate) fn subject_query(
    kind: SubjectHistoryKind,
    id: u64,
    scope: super::ScopeId,
    filter: &SubjectFilter,
) -> HistoryQuery {
    let fields = match kind {
        SubjectHistoryKind::Attack => SubjectFields::Attack(Binding::new(id, scope)),
        SubjectHistoryKind::Evolve => SubjectFields::Evolve(Binding::new(id, scope)),
        SubjectHistoryKind::Destroy => SubjectFields::Destroy(Binding::new(id, scope)),
        SubjectHistoryKind::Leave => SubjectFields::Leave(Binding::new(id, scope)),
        SubjectHistoryKind::Cemetery => SubjectFields::Cemetery(Binding::new(id, scope)),
        SubjectHistoryKind::Hand => SubjectFields::Hand(Binding::new(id, scope)),
    };
    let predicate = filter.predicate(fields);
    let pattern = match kind {
        SubjectHistoryKind::Attack => EventPattern::Attack {
            binding: Binding::new(id, scope),
            predicate,
        },
        SubjectHistoryKind::Evolve => EventPattern::Evolve {
            binding: Binding::new(id, scope),
            predicate,
        },
        SubjectHistoryKind::Destroy => EventPattern::Destroy {
            binding: Binding::new(id, scope),
            predicate,
        },
        SubjectHistoryKind::Leave | SubjectHistoryKind::Cemetery | SubjectHistoryKind::Hand => {
            EventPattern::LeaveField {
                binding: Binding::new(id, scope),
                predicate,
            }
        }
    };
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

fn subject_case(
    query: &HistoryQuery,
    kind: SubjectHistoryKind,
    fields: SubjectFields,
    predicate: &Expr<Bool>,
    key: (super::ScopeId, u64),
) -> Option<(SubjectHistoryKind, SubjectFilter)> {
    let filter = SubjectFilter::parse(fields, predicate)?;
    (!filter.own_turn
        && filter.keyword.is_none()
        && *query == subject_query(kind, key.1, key.0, &filter))
    .then_some((kind, filter))
}

pub(crate) fn subject_plan(query: &HistoryQuery) -> Option<(SubjectHistoryKind, SubjectFilter)> {
    if let EventPattern::Attack { binding, predicate } = &query.pattern {
        return subject_case(
            query,
            SubjectHistoryKind::Attack,
            SubjectFields::Attack(*binding),
            predicate,
            binding.key(),
        );
    }
    if let EventPattern::Evolve { binding, predicate } = &query.pattern {
        return subject_case(
            query,
            SubjectHistoryKind::Evolve,
            SubjectFields::Evolve(*binding),
            predicate,
            binding.key(),
        );
    }
    if let EventPattern::Destroy { binding, predicate } = &query.pattern {
        return subject_case(
            query,
            SubjectHistoryKind::Destroy,
            SubjectFields::Destroy(*binding),
            predicate,
            binding.key(),
        );
    }
    if let EventPattern::LeaveField { binding, predicate } = &query.pattern {
        for (kind, fields) in [
            (
                SubjectHistoryKind::Cemetery,
                SubjectFields::Cemetery(*binding),
            ),
            (SubjectHistoryKind::Hand, SubjectFields::Hand(*binding)),
            (SubjectHistoryKind::Leave, SubjectFields::Leave(*binding)),
        ] {
            if let Some(plan) = subject_case(query, kind, fields, predicate, binding.key()) {
                return Some(plan);
            }
        }
    }
    None
}

pub(crate) fn life_query(
    event: Binding<super::LifeChangeEvent>,
    filter: super::events::life::LifeFilter,
) -> HistoryQuery {
    HistoryQuery {
        pattern: EventPattern::LifeChange {
            binding: event,
            predicate: filter.predicate(event),
        },
        window: HistoryWindow {
            range: HistoryRange::CurrentTurn {},
            cutoff: HistoryCutoff::CurrentPoint {},
            relative_player: None,
        },
        include_execution: InclusionPolicy::IncludeAll {},
    }
}

fn life_filter(query: &HistoryQuery) -> Option<super::events::life::LifeFilter> {
    let EventPattern::LifeChange { binding, predicate } = &query.pattern else {
        return None;
    };
    let filter = super::events::life::LifeFilter::parse(*binding, predicate)?;
    (life_query(*binding, filter) == *query).then_some(filter)
}

pub(crate) fn life_plan(query: &HistoryQuery) -> Option<super::events::life::LifeFilter> {
    life_filter(query).filter(|filter| {
        filter.own.is_some() && filter.up == Some(false) && filter.own_turn.is_none()
    })
}

pub(crate) fn life_sum_plan(
    query: &super::NumericHistoryQuery,
) -> Option<super::events::life::LifeFilter> {
    let super::NumericHistoryQuery::LifeDelta {
        event,
        predicate,
        window,
        include_execution,
    } = query
    else {
        return None;
    };
    let filter = life_filter(&HistoryQuery {
        pattern: EventPattern::LifeChange {
            binding: *event,
            predicate: predicate.clone(),
        },
        window: window.clone(),
        include_execution: *include_execution,
    })?;
    (filter.up == Some(true) && filter.own.is_some() && filter.own_turn.is_none()).then_some(filter)
}

/// The event-time exception is one public facts query, not a general live snapshot.
pub(crate) fn evolution_count(read: &super::NatRead) -> Option<&HistoryQuery> {
    let super::NatRead::HistoryCount {
        query,
        count: super::HistoryCount::Facts,
    } = read
    else {
        return None;
    };
    let EventPattern::Evolve { binding, .. } = &query.pattern else {
        return None;
    };
    (*query
        == subject_query(
            SubjectHistoryKind::Evolve,
            binding.key().1,
            binding.key().0,
            &SubjectFilter {
                own: Some(true),
                kind: Some(super::CardKind::Follower),
                ..SubjectFilter::default()
            },
        ))
    .then_some(query)
}

pub(crate) fn evolution_trigger(trigger: &super::TriggerSpec) -> bool {
    let super::TriggerCondition::Event {
        pattern: EventPattern::Evolve { binding, predicate },
    } = &trigger.on
    else {
        return false;
    };
    trigger.limit.is_none()
        && trigger.active_zones == [super::ZoneKind::Field]
        && trigger.aggregation == (super::TriggerAggregation::PerFact {})
        && trigger.view == super::EventSnapshotPolicy::AfterEntry
        && *predicate
            == (SubjectFilter {
                own: Some(true),
                kind: Some(super::CardKind::Follower),
                ..SubjectFilter::default()
            })
            .predicate(SubjectFields::Evolve(*binding))
}

pub(crate) fn evolution_capture_query(
    capture: &super::Capture,
    expected: super::CaptureTime,
) -> Option<&HistoryQuery> {
    if !matches!(
        expected,
        super::CaptureTime::AtEvent {
            event: super::EventContext::Trigger {},
            ..
        }
    ) {
        return None;
    }
    let (at, value) = if let super::Capture::Integer {
        at,
        value: Expr::Compute(operation),
        ..
    } = capture
    {
        let super::IntOperation::FromNat { value } = operation.as_ref() else {
            return None;
        };
        (at, value)
    } else if let super::Capture::Natural { at, value, .. } = capture {
        (at, value)
    } else {
        return None;
    };
    let Expr::Live(read) = value else { return None };
    (*at == expected).then(|| evolution_count(read)).flatten()
}

pub(crate) fn evolution_capture_header(ability: &super::Ability) -> bool {
    ability.qualification == super::AbilityQualification::Ordinary
        && ability.play.is_none()
        && ability.kind == super::AbilityKind::Triggered
        && matches!(ability.source.origin, super::AbilityOrigin::Printed { .. })
        && ability.active_zones == [super::ZoneKind::Field]
        && ability.source.holder == (super::ObjectOperand::SourceHolder {})
        && ability.source.controller == (PlayerRef::Controller {})
        && ability.trigger.as_ref().is_some_and(|trigger| {
            evolution_trigger(trigger)
                && ability.event_captures.iter().all(|capture| {
                    evolution_capture_query(
                        capture,
                        super::CaptureTime::AtEvent {
                            event: super::EventContext::Trigger {},
                            policy: trigger.view,
                        },
                    )
                    .is_some()
                })
        })
}
