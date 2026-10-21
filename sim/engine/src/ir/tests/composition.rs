use super::*;

fn condition(predicate: Predicate) -> Expr<Bool> {
    Expr::Compute(Box::new(predicate))
}
fn controlled(control: Control) -> Located<Node> {
    Located {
        source: source("synthetic"),
        node: Node::Control { control },
    }
}
fn source_objects() -> Selector {
    Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    }
}
fn damage(recipients: RecipientSet, amount: i64, receipt: u64) -> Located<Node> {
    instruction(
        Action::Damage {
            recipients,
            amount: DamageAmount::Uniform {
                amount: Expr::Literal(Int(amount)),
            },
            flags: DamageFlags::Ability,
            source: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            batch: BatchPolicy::Instruction {},
        },
        receipt,
        "synthetic",
    )
}
fn batch_member(action: Action, receipt: u64) -> Located<Instruction> {
    let node = instruction(action, receipt, "synthetic");
    let Node::Instruction { instruction } = node.node else {
        panic!("instruction helper must produce an instruction")
    };
    Located {
        node: *instruction,
        source: node.source,
    }
}
fn assert_not_covered(simple: &Located<CardProgram>, special: &Located<CardProgram>) {
    assert!(matches!(
        Registry::test_support(simple).check_capabilities(special),
        Err(AdmissionError::Missing(_))
    ));
}
#[test]
fn simultaneous_move_and_token_share_capacity_and_release_receipts_together() {
    // BP13-SL16 needs both producers in one capacity transaction before its follow-up.
    let members = vec![
        batch_member(
            Action::Move {
                objects: source_objects(),
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Ex,
                        },
                    },
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
                reason: MoveReason::Put,
            },
            1,
        ),
        batch_member(
            Action::Token {
                tokens: vec![TokenSpec {
                    template: TokenTemplateId("synthetic".into()),
                    name: Expr::Literal(CardName("synthetic".into())),
                    count: Expr::Literal(Nat(1)),
                }],
                destination: TokenDestination::Single {
                    zone: Expr::Literal(ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex,
                    }),
                },
                output: Binding::new(2, ScopeId(1)),
                initial_state: None,
                initial_posture: Posture::Standing,
                entry_effects: vec![],
            },
            2,
        ),
    ];
    let sequential = card(
        program(
            members
                .iter()
                .cloned()
                .map(|member| Located {
                    source: member.source,
                    node: Node::Instruction {
                        instruction: Box::new(member.node),
                    },
                })
                .collect(),
        ),
        "synthetic",
    );
    let seed = card(
        program(vec![
            controlled(Control::Simultaneous {
                batch: InstructionBatch {
                    members,
                    membership: BatchMembership::AtBatchStart,
                    choice_stages: vec![],
                    commit: BatchCommit::SharedCapacityAtomic,
                    event_views: BatchViews::BeforeAndAfter,
                },
                receipt: ReceiptId(3),
            }),
            instruction(
                Action::Reveal {
                    objects: Selector::ReceiptMembers {
                        receipt: ObjectReceipt::Batch {
                            receipt: ReceiptReference::new(ReceiptId(3)),
                        },
                        projection: MemberProjection::After,
                    },
                },
                4,
                "synthetic",
            ),
        ]),
        "synthetic",
    );
    for code in [
        "control.simultaneous",
        "transaction.shared-capacity",
        "shape.ObjectReceipt.Batch",
        "action.token",
        "action.move",
    ] {
        assert_missing(&seed, code, "synthetic");
    }
    assert_not_covered(&sequential, &seed);
}
#[test]
fn event_header_and_subject_follow_keep_the_triggering_move_correlated() {
    let event = Binding::new(10, ScopeId(1));
    let event_context = EventContext::LeaveField { event };
    let movement = MovementEvent::LeaveField { event };
    let mut seed = card(
        program(vec![instruction(
            Action::Move {
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::FollowEventMove { event: movement }],
                },
                movement: MoveSpec {
                    to: MoveDestination::Owners {
                        zone: OwnerZone::Hand,
                    },
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
                reason: MoveReason::Return,
            },
            1,
            "synthetic",
        )]),
        "synthetic",
    );
    let ability = &mut seed.node.abilities.first_mut().unwrap().node;
    ability.kind = AbilityKind::Triggered;
    ability.trigger = Some(source_trigger(EventPattern::LeaveField {
        binding: event,
        predicate: condition(Predicate::And {
            predicates: vec![
                condition(Predicate::ObjectEquals {
                    left: Expr::Frozen(Box::new(ObjectFrozen::LeaveFieldObjectBefore { event })),
                    right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
                }),
                condition(Predicate::EventCause {
                    event: event_context,
                    pattern: CausePattern::Effect {
                        source: EffectSource::Any {},
                    },
                }),
                condition(Predicate::PhaseEquals {
                    left: Expr::Frozen(Box::new(PhaseFields::HeaderPhase {
                        event: event_context,
                    })),
                    right: Expr::Literal(Phase::Main),
                }),
                condition(Predicate::PlayerEquals {
                    left: Expr::Frozen(Box::new(PlayerFrozen::HeaderActor {
                        event: event_context,
                    })),
                    right: Expr::Literal(PlayerRef::Controller {}),
                }),
            ],
        }),
    }));
    ability.trigger.as_mut().unwrap().view = EventSnapshotPolicy::BeforeExit;
    for code in [
        "read.event-header",
        "read.event-header.move-receipt",
        "reference.trigger-follow",
        "predicate.event-cause",
    ] {
        assert_missing(&seed, code, "synthetic");
    }
    assert!(!codes(&seed).contains(&"value.live"));
}
fn opponents() -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::Opponents,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: None,
        },
        text_quantifier: TextQuantifier::All,
    }
}
fn opponent_followers() -> Selector {
    let binding = Binding::new(1, ScopeId(2));
    Selector::Zone {
        players: opponents(),
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(condition(Predicate::KindEquals {
            left: Expr::Live(Box::new(KindRead::Kind {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding }),
                },
                view: CharacteristicView::Effective,
            })),
            right: Expr::Literal(CardKind::Follower),
        })),
    }
}
fn constraint(
    groups: Vec<SelectionGroup>,
    minimum: u64,
    maximum: Option<u64>,
) -> SelectionConstraint {
    SelectionConstraint {
        groups,
        cardinality: Cardinality {
            minimum: Nat(minimum),
            maximum: maximum.map(Nat),
        },
        distinct_by: None,
        disjoint: Disjointness::Disjoint,
        aggregate_bounds: vec![],
    }
}
#[test]
fn primary_and_remainder_target_roles_feed_distinct_damage_instructions() {
    // The primary role is mandatory only when the single selection is nonempty.
    let primary = Binding::new(2, ScopeId(1));
    let remainder = Binding::new(3, ScopeId(1));
    let roles = TargetRoleSpec {
        roles: vec![
            TargetRole {
                role: Nat(1),
                cardinality: RoleCardinality::IfNonempty { count: Nat(1) },
                binding: primary,
            },
            TargetRole {
                role: Nat(2),
                cardinality: RoleCardinality::Remainder {},
                binding: remainder,
            },
        ],
    };
    let choice = Decision {
        captures: vec![],
        id: DecisionId(1),
        actor: PlayerRef::Controller {},
        domain: ChoiceDomain::Objects {
            constraint: constraint(
                vec![SelectionGroup {
                    selector: opponent_followers(),
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: Some(Nat(3)),
                    },
                }],
                0,
                Some(3),
            ),
            binding: Binding::new(1, ScopeId(1)),
            target_roles: Some(roles),
        },
        kind: DecisionKind::PlayParameter {
            slot: PlaySlot::Targets,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        visibility: Visibility::Public {},
    };
    let seed = card(
        program(vec![
            Located {
                source: source("synthetic"),
                node: Node::Decision { decision: choice },
            },
            damage(
                RecipientSet::Objects {
                    objects: Selector::Bound { reference: primary },
                },
                5,
                1,
            ),
            damage(
                RecipientSet::Objects {
                    objects: Selector::Bound {
                        reference: remainder,
                    },
                },
                2,
                2,
            ),
        ]),
        "synthetic",
    );
    for code in [
        "decision.target-roles",
        "decision.role-if-nonempty",
        "decision.role-remainder",
        "recipient.objects",
        "shape.Selector.Bound",
    ] {
        assert_missing(&seed, code, "synthetic");
    }
}

#[test]
fn frozen_characters_and_captured_optional_values_have_consumers() {
    let event = Binding::new(1, ScopeId(1));
    let number = CaptureRef {
        binding: Binding::new(2, ScopeId(1)),
    };
    let capture = Located {
        source: source("synthetic"),
        node: Node::Capture {
            capture: Capture::NumberInfo {
                at: CaptureTime::AtEvent {
                    event: EventContext::Discard { event },
                    policy: EventSnapshotPolicy::AtReveal,
                },
                value: Expr::Frozen(Box::new(NumberFrozen::Characters {
                    characters: Expr::Frozen(Box::new(CharacteristicsFields::DiscardCharacters {
                        event,
                    })),
                    property: NumericProperty::Cost,
                })),
                into: number.clone(),
            },
        },
    };
    let predicate = condition(Predicate::And {
        predicates: vec![
            condition(Predicate::Equal {
                operands: Box::new(ValueEquality::InfoInt {
                    left: Expr::Captured(number),
                    right: Expr::Literal(Info::Present(Int(3))),
                }),
            }),
            condition(Predicate::Equal {
                operands: Box::new(ValueEquality::InfoCollaborationName {
                    left: Expr::Live(Box::new(NameRead::CollaborationName {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::SourceHolder {}),
                        },
                    })),
                    right: Expr::Literal(Info::AbsentByRules),
                }),
            }),
            condition(Predicate::HasTrait {
                traits: Expr::Frozen(Box::new(TraitsFrozen::Characters {
                    characters: Expr::Frozen(Box::new(CharacteristicsFields::DiscardCharacters {
                        event,
                    })),
                })),
                trait_id: TraitId("synthetic".into()),
            }),
        ],
    });
    let seed = card(
        program(vec![
            capture,
            controlled(Control::If {
                condition: predicate,
                then_body: program(vec![]),
                else_body: None,
            }),
        ]),
        "synthetic",
    );
    for code in [
        "read.event-characteristics",
        "shape.ValueEquality.InfoInt",
        "shape.ValueEquality.InfoCollaborationName",
        "value.capture",
    ] {
        assert_missing(&seed, code, "synthetic");
    }
}
