use super::phase_restrictions::affected_player;
use super::{
    Ability, AbilityKind, AbilityOrigin, ActionPattern, Binding, Bool, BoolRead, Duration,
    DurationEnd, Effect, EffectDefinitionId, Expr, ObjectOperand, ObjectRef, PermissionSubject,
    PlayerRef, Polarity, RecipientSet, SelectionView, Selector, TimeAnchor, ZoneKind,
};

pub(crate) struct AttackPermission<'effect> {
    pub attacker: &'effect Selector,
    pub target: Option<&'effect RecipientSet>,
    pub condition: &'effect Expr<Bool>,
    pub duration: &'effect Duration,
    pub query: Option<EffectDefinitionId>,
}

pub(crate) fn owner_field(binding: Binding<ObjectRef>) -> RecipientSet {
    let players = affected_player(PlayerRef::Owner {});
    RecipientSet::Union {
        sets: vec![
            RecipientSet::Leaders {
                players: players.clone(),
            },
            RecipientSet::Objects {
                objects: Selector::Zone {
                    players,
                    zone: ZoneKind::Field,
                    view: SelectionView::Current {},
                    binding,
                    // The attack action has already confined its object targets to followers.
                    predicate: Box::new(Expr::Literal(Bool(true))),
                },
            },
        ],
    }
}

pub(crate) fn is_owner_field(target: &RecipientSet) -> bool {
    let RecipientSet::Union { sets } = target else {
        return false;
    };
    let [
        RecipientSet::Leaders { .. },
        RecipientSet::Objects {
            objects: Selector::Zone { binding, .. },
        },
    ] = sets.as_slice()
    else {
        return false;
    };
    *target == owner_field(*binding)
}

pub(crate) fn projection(effect: &Effect) -> Option<AttackPermission<'_>> {
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
    if objects != attacker {
        return None;
    }
    let (attacker, query) = if let Selector::Filter {
        input,
        binding,
        predicate,
    } = attacker
        && let Expr::Live(read) = predicate.as_ref()
        && let BoolRead::LegalAttack {
            attacker: ObjectOperand::Bound { binding: queried },
            ignore_querying_clause,
        } = read.as_ref()
        && binding == queried
    {
        if !matches!(target, Some(RecipientSet::Leaders { .. })) {
            return None;
        }
        (input.as_ref(), Some(*ignore_querying_clause))
    } else {
        (attacker, None)
    };
    Some(AttackPermission {
        attacker,
        target: target.as_ref(),
        condition,
        duration,
        query,
    })
}

pub(crate) fn require_projection(effect: &Effect) -> Option<AttackPermission<'_>> {
    let Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        action: ActionPattern::Attack {
            attacker,
            target: None,
        },
        polarity: Polarity::Require,
        condition,
        duration,
        ..
    } = effect
    else {
        return None;
    };
    (objects == attacker).then_some(AttackPermission {
        attacker,
        target: None,
        condition,
        duration,
        query: None,
    })
}

pub(crate) fn is_required_static(ability: &Ability) -> bool {
    static_shape(ability, require_projection)
}

pub(crate) fn ward_projection(effect: &Effect) -> Option<AttackPermission<'_>> {
    let Effect::Permission {
        subject: PermissionSubject::Objects { objects },
        action: ActionPattern::IgnoreWard { attacker },
        polarity: Polarity::Allow,
        condition,
        duration,
        ..
    } = effect
    else {
        return None;
    };
    (objects == attacker
        && *attacker
            == (Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            }))
    .then_some(AttackPermission {
        attacker,
        target: None,
        condition,
        duration,
        query: None,
    })
}

pub(crate) fn is_static(ability: &Ability) -> bool {
    static_shape(ability, projection)
}

pub(crate) fn is_ward_static(ability: &Ability) -> bool {
    static_shape(ability, ward_projection)
}

fn static_shape(ability: &Ability, project: fn(&Effect) -> Option<AttackPermission<'_>>) -> bool {
    ability.kind == AbilityKind::Static
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && !ability.declarations.is_empty()
        && ability.declarations.iter().all(|effect| {
            project(effect).is_some_and(|shape| {
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
