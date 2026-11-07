//! Printed permissions and restrictions checked before committing a card play.
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
    declaration(source, scope, active_zones, condition, Polarity::Forbid)
}

pub(crate) fn cemetery_ability(
    source: AbilitySource,
    scope: ScopeId,
    condition: Expr<Bool>,
) -> Ability {
    declaration(
        source,
        scope,
        vec![ZoneKind::Cemetery],
        condition,
        Polarity::Allow,
    )
}

fn declaration(
    source: AbilitySource,
    scope: ScopeId,
    active_zones: Vec<ZoneKind>,
    condition: Expr<Bool>,
    polarity: Polarity,
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
            polarity,
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

pub(crate) struct CardPlayRule<'condition> {
    pub(crate) condition: &'condition Expr<Bool>,
    pub(crate) forbids: bool,
}

pub(crate) fn card_play_rule(ability: &Ability) -> Option<CardPlayRule<'_>> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
    {
        return None;
    }
    let [
        Effect::Permission {
            condition,
            polarity,
            ..
        },
    ] = ability.declarations.as_slice()
    else {
        return None;
    };
    let polarity = match (polarity, ability.active_zones.as_slice()) {
        (Polarity::Forbid, [ZoneKind::Ex] | [ZoneKind::Hand, ZoneKind::Ex]) => Polarity::Forbid,
        (Polarity::Allow, [ZoneKind::Cemetery]) => Polarity::Allow,
        _ => return None,
    };
    let forbids = polarity == Polarity::Forbid;
    let mut expected = declaration(
        ability.source.clone(),
        ability.scope,
        ability.active_zones.clone(),
        condition.clone(),
        polarity,
    );
    expected.id = ability.id;
    (*ability == expected).then_some(CardPlayRule { condition, forbids })
}
