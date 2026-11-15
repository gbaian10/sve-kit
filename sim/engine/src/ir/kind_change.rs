//! Card kind is a separate nonnumeric layer; source-counter conditions cannot depend on kind.
use super::{
    AbilitySource, Bool, CardKind, CharacteristicPatch, ContinuousLayer, Duration, DurationEnd,
    Effect, EffectDefinitionId, EffectTarget, Expr, Holder, IntOperation, NatRead, ObjectOperand,
    PlayerRef, Predicate, Selector, TimeAnchor, TimestampBasis, ZoneKind,
};

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Kind conditions are closed over source counters to avoid characteristic cycles."
)]
pub(crate) fn condition(value: &Expr<Bool>) -> bool {
    let Expr::Compute(predicate) = value else {
        return matches!(value, Expr::Literal(_));
    };
    match predicate.as_ref() {
        Predicate::And { predicates } | Predicate::Or { predicates } => {
            !predicates.is_empty() && predicates.iter().all(condition)
        }
        Predicate::Not { predicate } => condition(predicate),
        Predicate::CompareInt {
            left: Expr::Compute(left),
            right: Expr::Literal(_),
            ..
        } => {
            matches!(left.as_ref(), IntOperation::FromNat { value: Expr::Live(read) } if matches!(read.as_ref(), NatRead::Counter { holder: Holder::Object { object: ObjectOperand::SourceHolder {} }, .. }))
        }
        _ => false,
    }
}

pub(crate) fn declaration(
    source: AbilitySource,
    kind: CardKind,
    condition: Option<Expr<Bool>>,
    id: EffectDefinitionId,
) -> Effect {
    Effect::Continuous {
        id,
        source,
        target: EffectTarget::Dynamic {
            selector: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
        condition,
        patch: CharacteristicPatch::SetKind { kind },
        layer: ContinuousLayer::NonNumeric,
        timestamp: TimestampBasis::ZoneEntry {
            object: ObjectOperand::SourceHolder {},
        },
        active_zones: vec![ZoneKind::Field],
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
    }
}

pub(crate) fn projection(effect: &Effect) -> Option<(CardKind, Option<&Expr<Bool>>)> {
    let Effect::Continuous {
        source,
        patch:
            CharacteristicPatch::SetKind {
                kind: kind @ (CardKind::Follower | CardKind::Amulet),
            },
        condition: predicate,
        id,
        ..
    } = effect
    else {
        return None;
    };
    (source.holder == (ObjectOperand::SourceHolder {})
        && source.controller == (PlayerRef::Controller {})
        && predicate.as_ref().is_none_or(condition)
        && *effect == declaration(source.clone(), *kind, predicate.clone(), *id))
    .then_some((*kind, predicate.as_ref()))
}
