//! Printed EX permissions may replace a nonzero Choice count with any number of modes.
use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource,
    ActionPattern, Bool, Cardinality, Duration, DurationEnd, Effect, Expr, Nat, ObjectOperand,
    PermissionSubject, PlayerRef, PlayerRelation, PlayerSet, Polarity, RuleId, ScopeId,
    TextQuantifier, TimeAnchor, ZoneKind,
};

pub(crate) fn ability(source: AbilitySource, scope: ScopeId) -> Ability {
    Ability {
        id: AbilityId(scope.0),
        scope,
        source,
        kind: AbilityKind::Static,
        qualification: AbilityQualification::Ordinary,
        active_zones: vec![ZoneKind::Ex],
        event_captures: vec![],
        play: None,
        trigger: None,
        body: None,
        declarations: vec![Effect::Permission {
            subject: PermissionSubject::Players {
                players: PlayerSet {
                    relation: PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
            },
            action: ActionPattern::ChooseMode {
                cardinality: Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                },
            },
            polarity: Polarity::Allow,
            condition: Expr::Literal(Bool(true)),
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

pub(crate) fn is_static(candidate: &Ability) -> bool {
    if !matches!(candidate.source.origin, AbilityOrigin::Printed { .. })
        || candidate.source.holder != (ObjectOperand::SourceHolder {})
        || candidate.source.controller != (PlayerRef::Controller {})
    {
        return false;
    }
    let mut expected = ability(candidate.source.clone(), candidate.scope);
    expected.id = candidate.id;
    *candidate == expected
}
