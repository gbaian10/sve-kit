//! Cost permissions constrain nodes; composition and evaluation remain in the shared plan.

use super::super::card::selection::public_zone_selector;
use crate::ir::history_turn::HistoryPlan;
use crate::ir::{
    Arithmetic, Bool, BoolRead, ExecutionRef, Expr, HistoryCount, Int, IntOperation, IntRead, Nat,
    NatOperation, NatRead, ObjectProperty, PlayerRef, PlayerResource, Predicate, SelectionView,
    Selector, ZoneKind,
};

pub(super) fn admits_natural(value: &Expr<Nat>) -> bool {
    match value {
        Expr::Literal(_) => true,
        Expr::Live(read) => match read.as_ref() {
            NatRead::HistoryCount {
                query,
                count: HistoryCount::Facts,
            } => HistoryPlan::compile(query).is_some_and(|plan| plan.cost_facts()),
            NatRead::ZoneCount {
                selector:
                    Selector::Zone {
                        zone: ZoneKind::Banish,
                        view: SelectionView::Current {},
                        players,
                        predicate,
                        ..
                    },
            } if players.relative_to == (PlayerRef::Controller {})
                && **predicate == Expr::Literal(Bool(true)) =>
            {
                true
            }
            NatRead::ZoneCount { selector } => public_zone_selector(selector).is_ok(),
            NatRead::TurnOrdinal { .. } => true,
            _ => false,
        },
        Expr::Compute(operation) => matches!(operation.as_ref(),
            NatOperation::DistinctCount {
                objects,
                property: ObjectProperty::Names | ObjectProperty::Cost,
            } if public_zone_selector(objects).is_ok()),
        _ => false,
    }
}

pub(super) fn admits_integer(value: &Expr<Int>) -> bool {
    match value {
        Expr::Literal(_) => true,
        Expr::Live(read) => matches!(
            read.as_ref(),
            IntRead::PlayerResource {
                property: PlayerResource::Life | PlayerResource::PpMaximum,
                ..
            }
        ),
        Expr::Compute(operation) => matches!(
            operation.as_ref(),
            IntOperation::FromNat { .. }
                | IntOperation::Arithmetic {
                    operation: Arithmetic::Subtract
                        | Arithmetic::Multiply
                        | Arithmetic::FloorDivide
                        | Arithmetic::Minimum,
                    ..
                }
        ),
        _ => false,
    }
}

pub(super) fn admits_boolean(value: &Expr<Bool>, play_origin: bool) -> bool {
    match value {
        Expr::Literal(_) => true,
        Expr::Live(read) => {
            matches!(
                read.as_ref(),
                BoolRead::Crimson {
                    player: PlayerRef::Controller {}
                }
            ) || (play_origin
                && matches!(
                    read.as_ref(),
                    BoolRead::CardPlayOrigin {
                        execution: ExecutionRef::Current {},
                        zone: ZoneKind::Hand | ZoneKind::Ex
                    }
                ))
        }
        Expr::Compute(predicate) => match predicate.as_ref() {
            Predicate::And { .. }
            | Predicate::Or { .. }
            | Predicate::Not { .. }
            | Predicate::CompareNat {
                right: Expr::Literal(_),
                ..
            }
            | Predicate::CompareInt {
                right: Expr::Literal(_),
                ..
            }
            | Predicate::PlayerEquals {
                left: Expr::Literal(PlayerRef::Controller {}),
                right: Expr::Literal(PlayerRef::Active {}),
            } => true,
            Predicate::Exists { objects } => public_zone_selector(objects).is_ok(),
            _ => false,
        },
        _ => false,
    }
}
