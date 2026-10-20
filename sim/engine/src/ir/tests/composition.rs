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
fn own_objects(zone: ZoneKind) -> Selector {
    Selector::Zone {
        players: controller_set(),
        zone,
        view: SelectionView::Current {},
        binding: Binding::new(1, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
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
fn declaration(effect: Effect) -> Located<CardProgram> {
    let mut seed = card(program(vec![]), "synthetic");
    seed.node
        .abilities
        .first_mut()
        .unwrap()
        .node
        .declarations
        .push(effect);
    seed
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
                    to: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex,
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
                    to: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Hand,
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
#[test]
fn fused_as_material_compares_snapshot_identity_without_reading_live_state() {
    let event = Binding::new(1, ScopeId(1));
    let material = Binding::new(2, ScopeId(2));
    let predicate = condition(Predicate::AnySnapshot {
        snapshots: Expr::Frozen(Box::new(VecRuntimeObjectViewFields::FusedMaterials {
            event,
        })),
        object: material,
        predicate: condition(Predicate::ObjectEquals {
            left: Expr::Frozen(Box::new(ObjectFrozen::SnapshotObject {
                snapshot: Expr::Bound(material),
            })),
            right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
        }),
    });
    let mut seed = card(program(vec![]), "synthetic");
    seed.node.abilities.first_mut().unwrap().node.trigger =
        Some(source_trigger(EventPattern::Fused {
            binding: event,
            predicate,
        }));
    assert_missing(&seed, "read.snapshot-object", "synthetic");
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
fn all_recipients() -> RecipientSet {
    RecipientSet::Union {
        sets: vec![
            RecipientSet::Objects {
                objects: opponent_followers(),
            },
            RecipientSet::Leaders {
                players: opponents(),
            },
        ],
    }
}
#[test]
fn dynamic_recipients_work_for_damage_distribution_and_attack_restrictions() {
    let simple = card(
        program(vec![damage(
            RecipientSet::Explicit {
                recipients: vec![DamageRecipient::Leader {
                    player: PlayerRef::Controller {},
                }],
            },
            2,
            1,
        )]),
        "synthetic",
    );
    let mut seed = card(program(vec![damage(all_recipients(), 2, 1)]), "synthetic");
    let ability = &mut seed.node.abilities.first_mut().unwrap().node;
    ability.declarations.push(Effect::Permission {
        subject: PermissionSubject::Objects {
            objects: source_objects(),
        },
        action: ActionPattern::Attack {
            attacker: source_objects(),
            target: Some(all_recipients()),
        },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: turn(),
        rule: RuleId(1),
    });
    ability.play = Some(PlaySpec {
        captures: vec![],
        parameters: vec![Decision {
            captures: vec![],
            id: DecisionId(1),
            actor: PlayerRef::Controller {},
            domain: ChoiceDomain::Distribution {
                recipients: all_recipients(),
                total: Expr::Literal(Nat(5)),
                binding: Binding::new(2, ScopeId(1)),
            },
            kind: DecisionKind::PlayParameter {
                slot: PlaySlot::Distribution,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            },
            visibility: Visibility::Public {},
        }],
        costs: CostSpec::Ordered { steps: vec![] },
    });
    assert_missing(&seed, "recipient.union", "synthetic");
    assert_not_covered(&simple, &seed);
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
fn redirect(to: ZoneKind, position: Option<Position>) -> Located<CardProgram> {
    declaration(Effect::Replacement {
        id: EffectDefinitionId(1),
        source: ability_source(),
        pattern: ProposedEventPattern::LeavesZone {
            objects: source_objects(),
            from: ZoneKind::Field,
        },
        condition: Expr::Literal(Bool(true)),
        rewrite: EventRewrite::RedirectMove {
            to: ZoneRef {
                player: PlayerRef::Controller {},
                kind: to,
            },
            position,
        },
        scope: ReplacementScope::SelfOnly,
        affected_player: PlayerRef::Controller {},
        once_per_event: OncePerEvent::Enforced,
        usage: None,
        prospective_chars: None,
        duration: turn(),
    })
}
#[test]
fn leave_field_replacement_can_redirect_any_destination_to_deck_top_or_banish() {
    let banish = redirect(ZoneKind::Banish, None);
    let deck = redirect(ZoneKind::Deck, Some(Position::Top {}));
    for seed in [&banish, &deck] {
        assert_missing(seed, "replacement.move.redirect", "synthetic");
        assert_missing(seed, "shape.ProposedEventPattern.LeavesZone", "synthetic");
    }
    assert_not_covered(&banish, &deck);
}
fn destroy_permission(cause: CausePattern) -> Located<CardProgram> {
    declaration(Effect::Permission {
        subject: PermissionSubject::Objects {
            objects: source_objects(),
        },
        action: ActionPattern::Destroy {
            objects: source_objects(),
            cause,
        },
        polarity: Polarity::Forbid,
        condition: Expr::Literal(Bool(true)),
        duration: turn(),
        rule: RuleId(1),
    })
}
#[test]
fn ability_only_destroy_prohibition_cannot_masquerade_as_rule_prohibition() {
    let ability = destroy_permission(CausePattern::Effect {
        source: EffectSource::Any {},
    });
    let rule = destroy_permission(CausePattern::AnyRule {});
    assert_not_covered(&ability, &rule);
    let binding = Binding::new(2, ScopeId(1));
    let mut opponents = controller_set();
    opponents.relation = PlayerRelation::Opponents;
    let opponent = destroy_permission(CausePattern::Effect {
        source: EffectSource::Matching {
            predicate: Box::new(SourcePredicate {
                binding,
                condition: condition(Predicate::PlayerIn {
                    player: Expr::Live(Box::new(PlayerRead::Role {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::Bound { binding }),
                        },
                        role: Role::Controller,
                    })),
                    players: opponents,
                }),
            }),
        },
    });
    assert_missing(&opponent, "predicate.effect-source", "synthetic");
    assert_not_covered(&ability, &opponent);
}
fn filtered_hand() -> Selector {
    let candidate = Binding::new(2, ScopeId(2));
    Selector::Filter {
        input: Box::new(own_objects(ZoneKind::Hand)),
        binding: candidate,
        predicate: Box::new(condition(Predicate::HasTrait {
            traits: Expr::Live(Box::new(TraitsRead::Traits {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding: candidate }),
                },
                view: CharacteristicView::Effective,
            })),
            trait_id: TraitId("synthetic".into()),
        })),
    }
}
#[test]
fn discard_any_and_disjoint_cost_groups_preserve_one_selection() {
    let filtered = filtered_hand();
    let group = |selector, maximum| SelectionGroup {
        selector,
        cardinality: Cardinality {
            minimum: Nat(0),
            maximum,
        },
    };
    let any = card(
        program(vec![instruction(
            Action::Discard {
                player: PlayerRef::Controller {},
                mode: DiscardMode::Chosen {
                    constraint: constraint(vec![group(filtered.clone(), None)], 0, None),
                },
            },
            1,
            "synthetic",
        )]),
        "synthetic",
    );
    let one = card(
        program(vec![instruction(
            Action::Discard {
                player: PlayerRef::Controller {},
                mode: DiscardMode::Chosen {
                    constraint: constraint(vec![group(filtered.clone(), Some(Nat(1)))], 1, Some(1)),
                },
            },
            1,
            "synthetic",
        )]),
        "synthetic",
    );
    assert_not_covered(&one, &any);
    assert_missing(&any, "cardinality.unbounded", "synthetic");
    let materials = Binding::new(3, ScopeId(1));
    let mut cost = batch_member(
        Action::Discard {
            player: PlayerRef::Controller {},
            mode: DiscardMode::Chosen {
                constraint: constraint(
                    vec![
                        SelectionGroup {
                            selector: source_objects(),
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                        },
                        SelectionGroup {
                            selector: filtered,
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                        },
                    ],
                    2,
                    Some(2),
                ),
            },
        },
        1,
    )
    .node;
    cost.context = InstructionContext::Cost;
    let grouped = card(
        program(vec![controlled(Control::Pay {
            at: PaymentTime::PlayParameter,
            costs: CostSpec::Ordered {
                steps: vec![CostStep {
                    instruction: Box::new(cost),
                    materials: Some(materials),
                }],
            },
            optional: PaymentOptionality::Required,
            on_paid: program(vec![instruction(
                Action::Reveal {
                    objects: Selector::Bound {
                        reference: materials,
                    },
                },
                2,
                "synthetic",
            )]),
            receipt: ReceiptId(3),
        })]),
        "synthetic",
    );
    for code in [
        "discard.chosen",
        "internal.Disjointness.Disjoint",
        "predicate.has-trait",
        "shape.Selector.Bound",
    ] {
        assert_missing(&grouped, code, "synthetic");
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

#[test]
fn repeated_quantity_results_can_drive_the_next_instruction() {
    let totals = Binding::new(3, ScopeId(1));
    let repeated = controlled(Control::Repeat {
        body: RepeatBody::Program {
            body: program(vec![instruction(
                Action::Draw {
                    player: PlayerRef::Controller {},
                    count: Expr::Literal(Int(1)),
                    mode: StepwiseMode::Exact,
                },
                1,
                "synthetic",
            )]),
        },
        termination: Termination::Count {
            count: Expr::Literal(Nat(3)),
        },
        check: IterationCheck::BeforeIteration,
        scope: ScopeId(2),
        receipts: vec![ReceiptExport::CardQuantity {
            source: ReceiptReference::new(ReceiptId(1)),
            iteration: Binding::new(2, ScopeId(2)),
            collect: Some(totals),
        }],
    });
    let follow = instruction(
        Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Compute(Box::new(NatOperation::ReceiptActualSum {
                    receipts: MeasuredReceiptList::Cards { receipts: totals },
                })),
            })),
            mode: StepwiseMode::Exact,
        },
        2,
        "synthetic",
    );
    let seed = card(program(vec![repeated, follow]), "synthetic");
    assert_missing(&seed, "read.receipt.actual-sum", "synthetic");
    assert_missing(&seed, "control.repeat.export", "synthetic");
}
