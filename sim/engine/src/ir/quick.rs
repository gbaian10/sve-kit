//! Exact printed Quick shapes shared by well-formedness and runtime admission.
use super::{
    Ability, AbilityKind, AbilityOrigin, AbilityQualification, ActionPattern, Bool, DurationEnd,
    Effect, Expr, KeywordId, ObjectOperand, PermissionSubject, PlayerRef, Polarity, Selector,
    TimeAnchor, ZoneKind,
};

pub(crate) fn is_printed_quick(ability: &Ability) -> bool {
    matches!(&ability.source.origin, AbilityOrigin::Keyword { keyword: KeywordId::Quick, origin }
        if matches!(origin.as_ref(), AbilityOrigin::Printed { .. }))
}

pub(crate) fn card_permission(ability: &Ability) -> bool {
    if !is_printed_quick(ability)
        || ability.kind != AbilityKind::Static
        || ability.qualification != AbilityQualification::Ordinary
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.active_zones.len() != 2
        || !ability.active_zones.contains(&ZoneKind::Hand)
        || !ability.active_zones.contains(&ZoneKind::Ex)
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
    {
        return false;
    }
    let [
        Effect::Permission {
            subject: PermissionSubject::Objects { objects: subject },
            action: ActionPattern::PlayCard { objects },
            polarity: Polarity::Allow,
            condition: Expr::Literal(Bool(true)),
            duration,
            ..
        },
    ] = ability.declarations.as_slice()
    else {
        return false;
    };
    let holder = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    *subject == holder
        && *objects == holder
        && duration.end == (DurationEnd::NoTemporalExpiry {})
        && duration.anchor == (TimeAnchor::Installation {})
        && duration.usage.is_none()
        && duration.intervals.is_empty()
}
