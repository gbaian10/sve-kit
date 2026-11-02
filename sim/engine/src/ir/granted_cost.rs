//! Closed self play-cost declarations reuse the common cost layer.
use super::{
    CostChange, CostKind, DurationEnd, Effect, Expr, Int, ObjectOperand, Selector, TimeAnchor,
};

pub(crate) fn delta(effect: &Effect) -> Option<i64> {
    let Effect::CostModifier {
        kind: CostKind::Play,
        subjects: Selector::Objects { objects },
        change: CostChange::Delta {
            amount: Expr::Literal(Int(amount)),
        },
        duration,
        usage: None,
    } = effect
    else {
        return None;
    };
    (*amount <= 0
        && objects == &[ObjectOperand::SourceHolder {}]
        && duration.end == (DurationEnd::NoTemporalExpiry {})
        && duration.anchor == (TimeAnchor::Installation {})
        && duration.usage.is_none()
        && duration.intervals.is_empty())
    .then_some(*amount)
}
