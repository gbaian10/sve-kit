//! Closed printed restrictions checked before committing a card play.
use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource,
    ActionPattern, Bool, Duration, DurationEnd, Effect, Expr, ObjectOperand, PermissionSubject,
    PlayerRef, Polarity, Predicate, RuleId, ScopeId, Selector, TimeAnchor, ZoneKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Restriction {
    FromEx,
    OwnTurn,
}

impl Restriction {
    pub(crate) fn ability(self, source: AbilitySource, scope: ScopeId) -> Ability {
        let objects = Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        };
        let (active_zones, condition) = match self {
            Self::FromEx => (vec![ZoneKind::Ex], Expr::Literal(Bool(true))),
            Self::OwnTurn => (
                vec![ZoneKind::Hand, ZoneKind::Ex],
                Expr::Compute(Box::new(Predicate::PlayerEquals {
                    left: Expr::Literal(PlayerRef::Controller {}),
                    right: Expr::Literal(PlayerRef::Active {}),
                })),
            ),
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
}

pub(crate) fn card_restriction(ability: &Ability) -> Option<Restriction> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
    {
        return None;
    }
    [Restriction::FromEx, Restriction::OwnTurn]
        .into_iter()
        .find(|restriction| {
            let mut expected = restriction.ability(ability.source.clone(), ability.scope);
            expected.id = ability.id;
            *ability == expected
        })
}
