//! Closed named printed abilities consumed by the corresponding rule procedures.
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

fn has_keyword(object: ObjectOperand, keyword: KeywordId) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::HasAbility {
        abilities: Expr::Live(Box::new(AbilitiesRead::Abilities {
            object: view(object),
            view: CharacteristicView::Effective,
        })),
        predicate: Box::new(AbilityPredicate {
            kind: None,
            keyword: Some(keyword),
            except: vec![],
            holder: None,
            origin: None,
        }),
    }))
}

fn ward(scope: ScopeId) -> (ActionPattern, Expr<Bool>, RuleId) {
    let own = followers(PlayerRelation::SelfPlayer, Binding::new(0, scope));
    let binding = Binding::new(1, scope);
    // Official Q819: an acted Ward that also has Intimidate creates no guard obligation.
    let wards = Selector::Filter {
        input: Box::new(own.clone()),
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                acted(ObjectOperand::Bound { binding }),
                has_keyword(ObjectOperand::Bound { binding }, KeywordId::Ward),
                Expr::Compute(Box::new(Predicate::Not {
                    predicate: has_keyword(ObjectOperand::Bound { binding }, KeywordId::Intimidate),
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
        Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                acted(ObjectOperand::SourceHolder {}),
                Expr::Compute(Box::new(Predicate::Not {
                    predicate: has_keyword(ObjectOperand::SourceHolder {}, KeywordId::Intimidate),
                })),
            ],
        })),
        RuleId(128),
    )
}

/// Build a complete named definition. Intrinsic combat keywords are interpreted
/// by rule procedures; they do not install a generic permission or triggered body.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only the closed complete printed keyword shapes are supported."
)]
pub(crate) fn definition(
    keyword: KeywordId,
    face: FaceId,
    line: Nat,
    id: AbilityId,
    scope: ScopeId,
) -> Option<Ability> {
    let permission = match keyword {
        KeywordId::Ward => Some(ward(scope)),
        KeywordId::Aura => Some((
            ActionPattern::ChooseTarget {
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
            },
            Expr::Literal(Bool(true)),
            RuleId(1215),
        )),
        KeywordId::Storm
        | KeywordId::Rush
        | KeywordId::Bane
        | KeywordId::Intimidate
        | KeywordId::Assail => None,
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
        declarations: permission
            .into_iter()
            .map(|(action, condition, rule)| Effect::Permission {
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
            })
            .collect(),
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
    let expected = if matches!(keyword, KeywordId::SingleDrive | KeywordId::TwinDrive) {
        let step = ability.body.as_ref()?.steps.first()?;
        let super::Node::Instruction { instruction } = &step.node else {
            return None;
        };
        drive_definition(
            *keyword,
            ability.id,
            ability.scope,
            instruction.id,
            &step.source,
        )
    } else {
        definition(*keyword, face.clone(), *line, ability.id, ability.scope)
    };
    (expected.as_ref() == Some(ability)).then_some(*keyword)
}

/// Expand the complete printed Drive rule into the ordinary attack subscription.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only the complete printed Single and Twin Drive definitions are expanded here."
)]
pub(crate) fn drive_definition(
    keyword: KeywordId,
    id: AbilityId,
    scope: ScopeId,
    instruction: super::InstructionId,
    span: &super::SourceSpan,
) -> Option<Ability> {
    use super::{
        Action, EventPattern, EventSnapshotPolicy, Instruction, InstructionContext, Located, Node,
        Program, ReceiptId, TriggerAggregation, TriggerCondition, TriggerSpec,
    };
    let repetitions = match keyword {
        KeywordId::SingleDrive => Nat(1),
        KeywordId::TwinDrive => Nat(2),
        _ => return None,
    };
    let source = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin: AbilityOrigin::Keyword {
            keyword,
            origin: Box::new(AbilityOrigin::Printed {
                face: FaceId(span.face.clone()),
                line: Nat(u64::from(span.line)),
            }),
        },
        controller: PlayerRef::Controller {},
    };
    let event = Binding::new(0, scope);
    Some(Ability {
        id,
        scope,
        source: source.clone(),
        kind: AbilityKind::Triggered,
        qualification: AbilityQualification::Ordinary,
        active_zones: vec![ZoneKind::Field],
        event_captures: vec![],
        play: None,
        trigger: Some(TriggerSpec {
            active_zones: vec![ZoneKind::Field],
            view: EventSnapshotPolicy::AfterEntry,
            on: TriggerCondition::Event {
                pattern: EventPattern::Attack {
                    binding: event,
                    predicate: super::events::timing::attack_predicate(event),
                },
            },
            limit: None,
            aggregation: TriggerAggregation::PerFact {},
        }),
        body: Some(Program {
            scope,
            steps: vec![Located {
                source: span.clone(),
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: instruction,
                        actor: PlayerRef::Controller {},
                        source: source.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        receipt: ReceiptId(instruction.0),
                        action: Action::DriveCheck {
                            player: PlayerRef::Controller {},
                            repetitions,
                            source,
                        },
                    }),
                },
            }],
        }),
        declarations: vec![],
    })
}
