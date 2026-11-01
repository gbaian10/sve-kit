use super::{
    AbilitySource, Bool, DamageKind, DamageRecipient, Duration, Effect, EventRewrite, Expr,
    ObjectOperand, OncePerEvent, PlayerRef, PlayerSet, Predicate, ProposedEventPattern,
    RecipientPredicate, ReplacementScope, Selector, SourcePredicate,
};

pub(crate) enum Recipients<'effect> {
    Objects(&'effect Selector),
    Leaders(&'effect PlayerSet),
}

pub(crate) struct Prevention<'effect> {
    pub source: &'effect AbilitySource,
    pub recipients: Option<Recipients<'effect>>,
    pub damage_source: Option<&'effect Selector>,
    pub kind: DamageKind,
    pub condition: &'effect Expr<Bool>,
    pub rewrite: &'effect EventRewrite,
    pub duration: &'effect Duration,
}

fn recipient_filter(predicate: &RecipientPredicate) -> Option<Recipients<'_>> {
    let Expr::Compute(condition) = &predicate.condition else {
        return None;
    };
    if let Predicate::RecipientIsFollower {
        recipient: DamageRecipient::Bound { binding },
        objects,
    } = condition.as_ref()
        && *binding == predicate.binding
    {
        return Some(Recipients::Objects(objects));
    }
    if let Predicate::RecipientIsLeader {
        recipient: DamageRecipient::Bound { binding },
        players,
    } = condition.as_ref()
        && *binding == predicate.binding
    {
        return Some(Recipients::Leaders(players));
    }
    None
}

fn source_filter(predicate: &SourcePredicate) -> Option<&Selector> {
    let Expr::Compute(condition) = &predicate.condition else {
        return None;
    };
    let Predicate::MemberOf {
        object: ObjectOperand::Bound { binding },
        set,
    } = condition.as_ref()
    else {
        return None;
    };
    (*binding == predicate.binding).then_some(set)
}

pub(crate) fn projection(effect: &Effect) -> Option<Prevention<'_>> {
    let Effect::Replacement {
        source,
        pattern:
            ProposedEventPattern::Damage {
                recipient,
                source: damage_source,
                kind,
            },
        condition,
        rewrite,
        scope: ReplacementScope::General,
        affected_player: PlayerRef::ProposedDamageRecipient {},
        once_per_event: OncePerEvent::Enforced,
        usage: None,
        prospective_chars: None,
        duration,
        ..
    } = effect
    else {
        return None;
    };
    let supported = matches!(
        rewrite,
        EventRewrite::PreventDamage {
            recipient: None,
            source: None,
            kind: DamageKind::Any
        } | EventRewrite::AddDamage {
            amount: Expr::Literal(_)
        }
    ) || matches!(rewrite, EventRewrite::CapDamage { threshold: Expr::Literal(threshold), replacement: Expr::Literal(replacement) } if threshold.0 > replacement.0 && threshold.0 <= i64::MAX.unsigned_abs());
    if !supported {
        return None;
    }
    if recipient.is_none() && damage_source.is_none() {
        return None;
    }
    let recipients = match recipient {
        Some(predicate) => Some(recipient_filter(predicate)?),
        None => None,
    };
    let damage_source = match damage_source {
        Some(predicate) => Some(source_filter(predicate)?),
        None => None,
    };
    Some(Prevention {
        source,
        recipients,
        damage_source,
        kind: *kind,
        condition,
        rewrite,
        duration,
    })
}
