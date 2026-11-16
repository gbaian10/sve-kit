//! A live printed rule changes combat damage without changing power or ability damage.
use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Bool,
    Duration, DurationEnd, Effect, Expr, ObjectOperand, PlayerRef, RuleModifier, ScopeId, Selector,
    TimeAnchor, ZoneKind,
};

pub(crate) fn ability(
    source: AbilitySource,
    scope: ScopeId,
    objects: Selector,
    condition: Expr<Bool>,
) -> Ability {
    Ability {
        id: AbilityId(scope.0),
        scope,
        source: source.clone(),
        kind: AbilityKind::Static,
        qualification: AbilityQualification::Ordinary,
        active_zones: vec![ZoneKind::Field],
        event_captures: vec![],
        play: None,
        trigger: None,
        body: None,
        declarations: vec![Effect::RuleModifier {
            modifier: RuleModifier::BattleDamageBasis { objects },
            condition,
            duration: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            source,
        }],
    }
}

pub(crate) fn projection(candidate: &Ability) -> Option<(&Selector, &Expr<Bool>)> {
    let [
        Effect::RuleModifier {
            modifier: RuleModifier::BattleDamageBasis { objects },
            condition,
            ..
        },
    ] = candidate.declarations.as_slice()
    else {
        return None;
    };
    if !matches!(candidate.source.origin, AbilityOrigin::Printed { .. })
        || candidate.source.holder != (ObjectOperand::SourceHolder {})
        || candidate.source.controller != (PlayerRef::Controller {})
    {
        return None;
    }
    let mut expected = ability(
        candidate.source.clone(),
        candidate.scope,
        objects.clone(),
        condition.clone(),
    );
    expected.id = candidate.id;
    (*candidate == expected).then_some((objects, condition))
}
