//! A printed holder restriction affects only its controller's rule-owned start stand.
use super::{
    Ability, AbilityKind, AbilityOrigin, ActionPattern, Bool, Duration, DurationEnd, Effect, Expr,
    ObjectOperand, PermissionSubject, PlayerRef, Polarity, RuleId, Selector, StandScope,
    TimeAnchor,
};

fn declaration(effect: &Effect) -> bool {
    matches!(effect, Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        action: ActionPattern::Stand { objects: action_objects, scope: StandScope::RuleStart { player: PlayerRef::Controller {} } },
        polarity: Polarity::Forbid, condition: Expr::Literal(Bool(true)), duration, rule: RuleId(723),
    } if objects == action_objects
        && matches!(objects, Selector::Objects { objects } if matches!(objects.as_slice(), [ObjectOperand::SourceHolder {}]))
        && *duration == (Duration { end: DurationEnd::NoTemporalExpiry {}, anchor: TimeAnchor::Installation {}, usage: None, intervals: vec![] }))
}

pub(crate) fn is_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability.declarations.iter().all(declaration)
}
