//! Current-turn queries share the closed public subject filters used by event headers.

use super::events::subject::{SubjectFields, SubjectFilter};
use super::{
    Binding, Bool, EventPattern, Expr, HistoryCutoff, HistoryQuery, HistoryRange, HistoryWindow,
    InclusionPolicy, PlayEvent, PlayerFrozen, PlayerRef, Predicate,
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
    (!filter.own_turn && *query == subject_query(kind, key.1, key.0, &filter))
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
