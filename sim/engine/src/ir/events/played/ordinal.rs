use crate::ir::history_turn::{HistoryPlan, PlayedHistoryFilter};
use crate::ir::{Bool, Comparison, Expr, HistoryCount, HistoryQuery, Nat, NatRead, Predicate};

#[derive(Clone, Debug)]
pub(crate) struct Ordinal {
    query: HistoryQuery,
    filter: PlayedHistoryFilter,
    number: Nat,
}

impl Ordinal {
    pub(crate) fn new(query: HistoryQuery, number: Nat) -> Option<Self> {
        let HistoryPlan::PlayedFiltered { filter, .. } = HistoryPlan::compile(&query)? else {
            return None;
        };
        (number.0 > 0).then_some(Self {
            query,
            filter,
            number,
        })
    }

    pub(crate) const fn number(&self) -> Nat {
        self.number
    }

    pub(crate) const fn filter(&self) -> &PlayedHistoryFilter {
        &self.filter
    }

    pub(crate) fn predicate(&self) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::CompareNat {
            left: Expr::Live(Box::new(NatRead::HistoryCount {
                query: self.query.clone(),
                count: HistoryCount::Facts,
            })),
            comparison: Comparison::Equal,
            right: Expr::Literal(self.number),
        }))
    }

    pub(crate) fn parse(expression: &Expr<Bool>) -> Option<Self> {
        let Expr::Compute(predicate) = expression else {
            return None;
        };
        let Predicate::CompareNat {
            left: Expr::Live(read),
            comparison: Comparison::Equal,
            right: Expr::Literal(number),
        } = predicate.as_ref()
        else {
            return None;
        };
        let NatRead::HistoryCount {
            query,
            count: HistoryCount::Facts,
        } = read.as_ref()
        else {
            return None;
        };
        Self::new(query.clone(), *number)
    }
}
