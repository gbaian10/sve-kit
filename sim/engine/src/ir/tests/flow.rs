use super::*;

fn controlled(control: Control) -> Located<CardProgram> {
    card(
        program(vec![Located {
            source: source("synthetic"),
            node: Node::Control { control },
        }]),
        "synthetic",
    )
}
fn move_to_ex() -> Located<Node> {
    instruction(
        Action::Move {
            objects: Selector::Ordered {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Deck,
                },
                start: Position::Top {},
                count: Expr::Literal(Nat(1)),
                direction: SliceDirection::Forward,
            },
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
        "synthetic",
    )
}
#[test]
fn repeat_exports_current_receipt_and_collects_all_successful_generations() {
    let current = Binding::new(1, ScopeId(2));
    let collected = Binding::new(2, ScopeId(1));
    let repeated = Control::Repeat {
        body: RepeatBody::Program {
            body: Program {
                scope: ScopeId(3),
                steps: vec![move_to_ex()],
            },
        },
        termination: Termination::Until {
            predicate: Expr::Compute(Box::new(Predicate::CompareNat {
                left: Expr::Live(Box::new(NatRead::ZoneCapacity {
                    player: PlayerRef::Controller {},
                    zone: CapacityZone::Ex,
                    property: CapacityProperty::Free,
                })),
                comparison: Comparison::Equal,
                right: Expr::Literal(Nat(0)),
            })),
        },
        check: IterationCheck::BeforeIteration,
        scope: ScopeId(2),
        receipts: vec![ReceiptExport::Move {
            source: ReceiptReference::new(ReceiptId(1)),
            iteration: current,
            collect: Some(collected),
        }],
    };
    let affected = Binding::new(3, ScopeId(4));
    let follow = instruction(
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects {
                objects: Selector::MoveAfterAll {
                    receipts: MovementReceiptList::Move {
                        receipts: collected,
                    },
                    permission: FollowPermission::SameEffectExplicitReference {},
                    deduplicate: GenerationDeduplication::ByObjectAndGeneration,
                },
                affected,
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::CostModifier {
                subjects: Selector::Bound {
                    reference: affected,
                },
                change: CostChange::Delta {
                    amount: Expr::Literal(Int(-1)),
                },
                duration: turn(),
                usage: None,
            }),
            duration: turn(),
        },
        2,
        "synthetic",
    );
    let mut seed = controlled(repeated.clone());
    for ability in &mut seed.node.abilities {
        if let Some(body) = &mut ability.node.body {
            body.steps.push(follow.clone());
        }
    }
    assert_missing(&seed, "control.repeat.export", "synthetic");
    assert_missing(&seed, "receipt.move-after-all", "synthetic");
    assert!(matches!(
        Registry::test_support(&controlled(repeated)).check_capabilities(&seed),
        Err(AdmissionError::Missing(_))
    ));
    let per_iteration = controlled(Control::Repeat {
        body: RepeatBody::Program {
            body: program(vec![move_to_ex()]),
        },
        termination: Termination::While {
            predicate: Expr::Compute(Box::new(Predicate::Exists {
                objects: Selector::MoveAfter {
                    receipt: MovementReceipt::Move {
                        receipt: ReceiptReference::iteration(current),
                    },
                    permission: FollowPermission::SameEffectExplicitReference {},
                },
            })),
        },
        check: IterationCheck::AfterIteration,
        scope: ScopeId(2),
        receipts: vec![ReceiptExport::Move {
            source: ReceiptReference::new(ReceiptId(1)),
            iteration: current,
            collect: None,
        }],
    });
    assert_missing(&per_iteration, "receipt.iteration", "synthetic");
}
#[test]
fn each_player_exposes_one_player_to_all_stages() {
    let player = Binding::new(1, ScopeId(2));
    let mut discard = instruction(
        Action::Discard {
            player: PlayerRef::Bound { binding: player },
            mode: DiscardMode::DownTo { count: Nat(2) },
        },
        1,
        "synthetic",
    );
    if let Node::Instruction { instruction } = &mut discard.node {
        instruction.actor = PlayerRef::Bound { binding: player };
    }
    let seed = controlled(Control::EachPlayer {
        players: PlayerSet {
            relation: PlayerRelation::All,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: None,
            },
            text_quantifier: TextQuantifier::All,
        },
        player,
        stages: vec![PlayerStage {
            body: Program {
                scope: ScopeId(2),
                steps: vec![discard],
            },
            mode: StageMode::ChoiceProcedure {},
        }],
        choice_order: PlayerOrder::TurnPlayerFirst,
    });
    assert_missing(&seed, "control.each-player", "synthetic");
    assert_missing(&seed, "shape.PlayerRef.Bound", "synthetic");
}
#[test]
fn optional_payment_requires_more_than_mandatory_payment_support() {
    let payment = |optional| {
        controlled(Control::Pay {
            at: PaymentTime::PlayParameter,
            costs: CostSpec::Ordered { steps: vec![] },
            optional,
            on_paid: program(vec![]),
            receipt: ReceiptId(1),
        })
    };
    let required = payment(PaymentOptionality::Required);
    let optional = payment(PaymentOptionality::Optional);
    let Err(AdmissionError::Missing(missing)) =
        Registry::test_support(&required).check_capabilities(&optional)
    else {
        panic!("optional payment was admitted as mandatory");
    };
    assert_eq!(missing.len(), 1);
    for item in missing {
        assert_eq!(item.capability.code, "internal.PaymentOptionality.Optional");
    }
}
#[test]
fn discard_successful_member_can_be_read_from_its_receipt_snapshot() {
    let member = Binding::new(1, ScopeId(2));
    let condition = Expr::Compute(Box::new(Predicate::ReceiptAny {
        receipt: ObjectReceipt::Movement {
            receipt: MovementReceipt::Discard {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
        },
        projection: MemberProjection::Before,
        member,
        predicate: Expr::Compute(Box::new(Predicate::HasTrait {
            traits: Expr::Frozen(Box::new(TraitsFrozen::Snapshot {
                object: Expr::Bound(member),
            })),
            trait_id: TraitId("synthetic".to_owned()),
        })),
    }));
    let seed = controlled(Control::If {
        condition,
        then_body: program(vec![]),
        else_body: None,
    });
    for code in [
        "read.receipt.member-any",
        "shape.MovementReceipt.Discard",
        "read.snapshot-characteristics",
    ] {
        assert_missing(&seed, code, "synthetic");
    }
}

#[test]
fn target_roles_unbounded_choices_and_declining_need_distinct_support() {
    let choice = |target_roles, maximum, may| {
        card(
            program(vec![Located {
                source: source("synthetic"),
                node: Node::Decision {
                    decision: Decision {
                        captures: vec![],
                        id: DecisionId(1),
                        actor: PlayerRef::Controller {},
                        domain: ChoiceDomain::Objects {
                            constraint: SelectionConstraint {
                                groups: vec![],
                                cardinality: Cardinality {
                                    minimum: Nat(0),
                                    maximum,
                                },
                                distinct_by: None,
                                disjoint: Disjointness::Disjoint,
                                aggregate_bounds: vec![],
                            },
                            binding: Binding::new(1, ScopeId(1)),
                            target_roles,
                        },
                        kind: DecisionKind::ResolutionChoice {
                            may,
                            cardinality: Cardinality {
                                minimum: Nat(0),
                                maximum,
                            },
                            precondition: None,
                        },
                        visibility: Visibility::Public {},
                    },
                },
            }]),
            "synthetic",
        )
    };
    let simple = choice(None, Some(Nat(1)), Declinable::Required);
    let role = choice(
        Some(TargetRoleSpec {
            roles: vec![TargetRole {
                role: Nat(1),
                cardinality: RoleCardinality::IfNonempty { count: Nat(1) },
                binding: Binding::new(2, ScopeId(1)),
            }],
        }),
        Some(Nat(1)),
        Declinable::Required,
    );
    assert_missing(&role, "decision.target-roles", "synthetic");
    let support = Registry::test_support(&simple);
    for special in [
        role,
        choice(None, None, Declinable::Required),
        choice(None, Some(Nat(1)), Declinable::MayDecline),
    ] {
        assert!(matches!(
            support.check_capabilities(&special),
            Err(AdmissionError::Missing(_))
        ));
    }
}
