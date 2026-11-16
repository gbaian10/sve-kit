//! Printed counter caps constrain additions while their field ability is effective.
use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Bool,
    CounterKind, Duration, DurationEnd, Effect, Expr, Holder, Nat, ObjectOperand, PlayerRef,
    RuleModifier, ScopeId, TimeAnchor, ZoneKind,
};

pub(crate) fn ability(
    source: AbilitySource,
    scope: ScopeId,
    kind: CounterKind,
    cap: Nat,
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
            modifier: RuleModifier::CounterCap {
                holder: Holder::Object {
                    object: ObjectOperand::SourceHolder {},
                },
                kind,
                cap,
            },
            condition: Expr::Literal(Bool(true)),
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

pub(crate) fn projection(candidate: &Ability) -> Option<(&CounterKind, Nat)> {
    let [
        Effect::RuleModifier {
            modifier: RuleModifier::CounterCap { kind, cap, .. },
            ..
        },
    ] = candidate.declarations.as_slice()
    else {
        return None;
    };
    if kind.0.is_empty()
        || i64::try_from(cap.0).is_err()
        || !matches!(candidate.source.origin, AbilityOrigin::Printed { .. })
        || candidate.source.holder != (ObjectOperand::SourceHolder {})
        || candidate.source.controller != (PlayerRef::Controller {})
    {
        return None;
    }
    let mut expected = ability(
        candidate.source.clone(),
        candidate.scope,
        kind.clone(),
        *cap,
    );
    expected.id = candidate.id;
    (*candidate == expected).then_some((kind, *cap))
}
