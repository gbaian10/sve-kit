use super::{
    Ability, AbilityKind, AbilityOrigin, ActionPattern, Bool, Duration, DurationEnd, Effect, Expr,
    PermissionSubject, Polarity, RecipientSet, Selector, TimeAnchor,
};

pub(crate) struct AttackRestriction<'effect> {
    pub attacker: &'effect Selector,
    pub target: Option<&'effect RecipientSet>,
    pub condition: &'effect Expr<Bool>,
    pub duration: &'effect Duration,
}

pub(crate) fn projection(effect: &Effect) -> Option<AttackRestriction<'_>> {
    let Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        action: ActionPattern::Attack { attacker, target },
        polarity: Polarity::Forbid,
        condition,
        duration,
        ..
    } = effect
    else {
        return None;
    };
    (objects == attacker).then_some(AttackRestriction {
        attacker,
        target: target.as_ref(),
        condition,
        duration,
    })
}

pub(crate) fn is_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability.declarations.iter().all(|effect| {
            projection(effect).is_some_and(|shape| {
                shape.duration
                    == &Duration {
                        end: DurationEnd::NoTemporalExpiry {},
                        anchor: TimeAnchor::Installation {},
                        usage: None,
                        intervals: vec![],
                    }
            })
        })
}
