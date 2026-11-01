//! Printed restrictions checked before committing a card play.
use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource,
    ActionPattern, Bool, Duration, DurationEnd, Effect, Expr, ObjectOperand, PermissionSubject,
    PlayerRef, Polarity, RuleId, ScopeId, Selector, TimeAnchor, ZoneKind,
};

pub(crate) fn ability(
    source: AbilitySource,
    scope: ScopeId,
    active_zones: Vec<ZoneKind>,
    condition: Expr<Bool>,
) -> Ability {
    let objects = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    Ability {
        id: AbilityId(scope.0),
        scope,
        source,
        kind: AbilityKind::Static,
        qualification: AbilityQualification::Ordinary,
        active_zones,
        event_captures: vec![],
        play: None,
        trigger: None,
        body: None,
        declarations: vec![Effect::Permission {
            subject: PermissionSubject::Objects {
                objects: objects.clone(),
            },
            action: ActionPattern::PlayCard { objects },
            polarity: Polarity::Forbid,
            condition,
            duration: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            rule: RuleId(133),
        }],
    }
}

pub(crate) fn card_restriction(ability: &Ability) -> Option<&Expr<Bool>> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || !matches!(
            ability.active_zones.as_slice(),
            [ZoneKind::Ex] | [ZoneKind::Hand, ZoneKind::Ex]
        )
    {
        return None;
    }
    let [Effect::Permission { condition, .. }] = ability.declarations.as_slice() else {
        return None;
    };
    let mut expected = self::ability(
        ability.source.clone(),
        ability.scope,
        ability.active_zones.clone(),
        condition.clone(),
    );
    expected.id = ability.id;
    (*ability == expected).then_some(condition)
}
