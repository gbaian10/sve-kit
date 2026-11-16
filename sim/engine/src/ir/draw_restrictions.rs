//! Printed draw restrictions retain the affected player and excluded phase.
use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource,
    ActionPattern, Cardinality, Duration, DurationEnd, Effect, Expr, Nat, ObjectOperand,
    PermissionSubject, Phase, PhaseRead, PlayerRef, PlayerRelation, PlayerSet, Polarity, Predicate,
    RuleId, ScopeId, TextQuantifier, TimeAnchor, ZoneKind,
};

pub(crate) fn ability(source: AbilitySource, scope: ScopeId, relation: PlayerRelation) -> Ability {
    let player = if relation == PlayerRelation::SelfPlayer {
        PlayerRef::Controller {}
    } else {
        super::events::timing::opponent()
    };
    Ability {
        id: AbilityId(scope.0),
        scope,
        source,
        kind: AbilityKind::Static,
        qualification: AbilityQualification::Ordinary,
        active_zones: vec![ZoneKind::Field],
        event_captures: vec![],
        play: None,
        trigger: None,
        body: None,
        declarations: vec![Effect::Permission {
            subject: PermissionSubject::Players {
                players: PlayerSet {
                    relation,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
            },
            action: ActionPattern::Draw { player },
            polarity: Polarity::Forbid,
            condition: Expr::Compute(Box::new(Predicate::Not {
                predicate: Expr::Compute(Box::new(Predicate::PhaseEquals {
                    left: Expr::Live(Box::new(PhaseRead::Phase {})),
                    right: Expr::Literal(Phase::Start),
                })),
            })),
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

pub(crate) fn projection(candidate: &Ability) -> Option<&PlayerRelation> {
    let [
        Effect::Permission {
            subject: PermissionSubject::Players { players },
            ..
        },
    ] = candidate.declarations.as_slice()
    else {
        return None;
    };
    if !matches!(
        players.relation,
        PlayerRelation::SelfPlayer | PlayerRelation::Opponents
    ) || !matches!(candidate.source.origin, AbilityOrigin::Printed { .. })
        || candidate.source.holder != (ObjectOperand::SourceHolder {})
        || candidate.source.controller != (PlayerRef::Controller {})
    {
        return None;
    }
    let mut expected = ability(candidate.source.clone(), candidate.scope, players.relation);
    expected.id = candidate.id;
    (*candidate == expected).then_some(&players.relation)
}
