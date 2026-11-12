use super::*;
use crate::ir::history_turn::{HistoryPlan, played_query};

fn query(pattern: EventPattern) -> HistoryQuery {
    let mut query = played_query(Binding::new(30, ScopeId(1)), None);
    query.pattern = pattern;
    query
}

fn count_query(query: HistoryQuery) -> Program {
    body(
        1,
        vec![draw(
            1,
            signed(Expr::Live(Box::new(NatRead::HistoryCount {
                query,
                count: HistoryCount::Facts,
            }))),
        )],
    )
}

#[test]
fn uncompiled_queries_keep_their_generic_validation_results() {
    let generic = query(EventPattern::Move {
        binding: Binding::new(30, ScopeId(1)),
        predicate: Expr::Literal(Bool(false)),
    });
    assert!(HistoryPlan::compile(&generic).is_none());
    accept(count_query(generic));
    for pattern in [
        EventPattern::Banish {
            binding: Binding::new(30, ScopeId(1)),
            predicate: Expr::Literal(Bool(false)),
        },
        EventPattern::UbActivated {
            binding: Binding::new(30, ScopeId(1)),
            predicate: Expr::Literal(Bool(false)),
        },
    ] {
        let query = query(pattern);
        assert!(HistoryPlan::compile(&query).is_none());
        reject(count_query(query), WellFormedReason::UnsupportedShape);
    }
}

#[test]
fn noncanonical_stat_discard_and_play_queries_still_fail_wellformed_validation() {
    for pattern in [
        EventPattern::StatChange {
            binding: Binding::new(30, ScopeId(1)),
            predicate: Expr::Literal(Bool(false)),
        },
        EventPattern::Discard {
            binding: Binding::new(30, ScopeId(1)),
            predicate: Expr::Literal(Bool(false)),
        },
        EventPattern::Play {
            binding: Binding::new(30, ScopeId(1)),
            predicate: Expr::Literal(Bool(false)),
        },
    ] {
        reject(
            count_query(query(pattern)),
            WellFormedReason::UnsupportedShape,
        );
    }
}
