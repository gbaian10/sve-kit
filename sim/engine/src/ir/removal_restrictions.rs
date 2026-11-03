//! The printed holder's destruction and banishment prohibitions.
use super::{
    Ability, AbilityKind, AbilityOrigin, ActionPattern, Bool, CausePattern, Duration, DurationEnd,
    Effect, EffectSource, Expr, ObjectOperand, PermissionSubject, Polarity, RuleId, Selector,
    TimeAnchor,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Removal {
    Destroy,
    Banish,
}

pub(crate) fn projection(effect: &Effect) -> Option<Removal> {
    let Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        action,
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration,
        rule: RuleId(133),
    } = effect
    else {
        return None;
    };
    if !matches!(objects, Selector::Objects { objects } if matches!(objects.as_slice(), [ObjectOperand::SourceHolder {}]))
        || *duration
            != (Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            })
    {
        return None;
    }
    let (affected, kind) = if let ActionPattern::Destroy {
        objects: affected,
        cause: CausePattern::Effect {
            source: EffectSource::Any {},
        },
    } = action
    {
        (affected, Removal::Destroy)
    } else if let ActionPattern::Banish {
        objects: affected,
        cause: CausePattern::Any {},
    } = action
    {
        (affected, Removal::Banish)
    } else {
        return None;
    };
    (affected == objects).then_some(kind)
}

pub(crate) fn is_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability
            .declarations
            .iter()
            .all(|effect| projection(effect).is_some())
}

fn holder_ability_prevention(effect: &Effect) -> bool {
    let Some(shape) = super::prevention::projection(effect) else {
        return false;
    };
    shape.kind == super::DamageKind::Ability
        && shape.damage_source.is_none()
        && matches!(shape.rewrite, super::EventRewrite::PreventDamage { .. })
        && matches!(shape.recipients, Some(super::prevention::Recipients::Objects(objects)) if *objects == (Selector::Objects { objects: vec![ObjectOperand::SourceHolder {}] }))
}

pub(crate) fn is_mixed_static(ability: &Ability) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && ability
            .declarations
            .iter()
            .any(|effect| projection(effect).is_some())
        && ability.declarations.iter().any(holder_ability_prevention)
        && ability
            .declarations
            .iter()
            .all(|effect| projection(effect).is_some() || holder_ability_prevention(effect))
}
