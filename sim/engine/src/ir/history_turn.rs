//! Public played-card counts have no subject filters or historical object reads.
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
