//! Closed printed Ward/Aura declarations; rule procedures are admitted separately.
use super::{
    AbilitiesRead, Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityPredicate,
    AbilityQualification, AbilitySource, ActionPattern, Binding, Bool, CardKind, Cardinality,
    CharacteristicView, Duration, DurationEnd, Effect, Expr, FaceId, KeywordId, KindRead, Nat,
    ObjectOperand, ObjectRef, ObjectView, PermissionSubject, PlayerRef, PlayerRelation, PlayerSet,
    Polarity, Posture, PostureRead, Predicate, RecipientSet, RuleId, ScopeId, SelectionView,
    Selector, TextQuantifier, TimeAnchor, ZoneKind,
};

const fn players(relation: PlayerRelation) -> PlayerSet {
    PlayerSet {
        relation,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::Implicit,
    }
}

fn view(object: ObjectOperand) -> ObjectView {
    ObjectView::Current {
        object: Box::new(object),
    }
}

fn acted(object: ObjectOperand) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::PostureEquals {
        left: Expr::Live(Box::new(PostureRead::Posture {
            object: view(object),
        })),
        right: Expr::Literal(Posture::Acted),
    }))
}

fn followers(relation: PlayerRelation, binding: Binding<ObjectRef>) -> Selector {
    Selector::Zone {
        players: players(relation),
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: view(ObjectOperand::Bound { binding }),
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        }))),
    }
}

fn ward(scope: ScopeId) -> (ActionPattern, Expr<Bool>, RuleId) {
    let own = followers(PlayerRelation::SelfPlayer, Binding::new(0, scope));
    let binding = Binding::new(1, scope);
    let wards = Selector::Filter {
        input: Box::new(own.clone()),
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                acted(ObjectOperand::Bound { binding }),
                Expr::Compute(Box::new(Predicate::HasAbility {
                    abilities: Expr::Live(Box::new(AbilitiesRead::Abilities {
                        object: view(ObjectOperand::Bound { binding }),
                        view: CharacteristicView::Effective,
                    })),
                    predicate: Box::new(AbilityPredicate {
                        kind: None,
                        keyword: Some(KeywordId::Ward),
                        except: vec![],
                        holder: None,
                        origin: None,
                    }),
                })),
            ],
        }))),
    };
    (
        ActionPattern::Attack {
            attacker: followers(PlayerRelation::Opponents, Binding::new(2, scope)),
            target: Some(RecipientSet::Union {
                sets: vec![
                    RecipientSet::Leaders {
                        players: players(PlayerRelation::SelfPlayer),
                    },
                    RecipientSet::Objects {
                        objects: Selector::Difference {
                            input: Box::new(own),
                            except: Box::new(wards),
                        },
                    },
                ],
            }),
        },
        acted(ObjectOperand::SourceHolder {}),
        RuleId(128),
    )
}

/// Build the complete, named permission shape. Ward's optional posture choices
/// belong to rule procedures, not an unconditional effect of this declaration.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only these two complete printed keyword shapes are supported."
)]
pub(crate) fn definition(
    keyword: KeywordId,
    face: FaceId,
    line: Nat,
    id: AbilityId,
    scope: ScopeId,
) -> Option<Ability> {
    let (action, condition, rule) = match keyword {
        KeywordId::Ward => ward(scope),
        KeywordId::Aura => (
            ActionPattern::ChooseTarget {
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
            },
            Expr::Literal(Bool(true)),
            RuleId(1215),
        ),
        _ => return None,
    };
    Some(Ability {
        id,
        scope,
        source: AbilitySource {
            holder: ObjectOperand::SourceHolder {},
            origin: AbilityOrigin::Keyword {
                keyword,
                origin: Box::new(AbilityOrigin::Printed { face, line }),
            },
            controller: PlayerRef::Controller {},
        },
        kind: AbilityKind::Static,
        qualification: AbilityQualification::Ordinary,
        active_zones: vec![ZoneKind::Field],
        event_captures: vec![],
        play: None,
        trigger: None,
        body: None,
        declarations: vec![Effect::Permission {
            subject: PermissionSubject::Players {
                players: players(PlayerRelation::Opponents),
            },
            action,
            polarity: Polarity::Forbid,
            condition,
            duration: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            rule,
        }],
    })
}

/// No nested/granted origin, partial expansion, or generic permission is admitted.
pub(crate) fn card_declaration(ability: &Ability) -> Option<KeywordId> {
    let AbilityOrigin::Keyword { keyword, origin } = &ability.source.origin else {
        return None;
    };
    let AbilityOrigin::Printed { face, line } = origin.as_ref() else {
        return None;
    };
    (definition(*keyword, face.clone(), *line, ability.id, ability.scope).as_ref() == Some(ability))
        .then_some(*keyword)
}
