use super::*;

fn body(scope: u64, steps: Vec<Located<Node>>) -> Program {
    Program {
        scope: ScopeId(scope),
        steps,
    }
}
fn control(control: Control) -> Located<Node> {
    Located {
        source: source("synthetic"),
        node: Node::Control { control },
    }
}
fn draw(id: u64, count: Expr<Int>) -> Located<Node> {
    instruction(
        Action::Draw {
            player: PlayerRef::Controller {},
            count,
            mode: StepwiseMode::Exact,
        },
        id,
        "synthetic",
    )
}
fn declare(id: u64, binding: Binding<Int>) -> Located<Node> {
    instruction(
        Action::Declare {
            actor: PlayerRef::Controller {},
            declaration: Declaration::BoundedInteger {
                minimum: Int(-2),
                maximum: Int(7),
                binding,
            },
        },
        id,
        "synthetic",
    )
}
fn capture(binding: Binding<Int>, value: Expr<Int>) -> Located<Node> {
    Located {
        source: source("synthetic"),
        node: Node::Capture {
            capture: Capture::Integer {
                at: CaptureTime::AtResolution { scope: ScopeId(1) },
                value,
                into: CaptureRef { binding },
            },
        },
    }
}
fn reject(program: Program, reason: WellFormedReason) -> WellFormedError {
    let error = WellFormedProgram::check(program).expect_err("invalid program passed validation");
    assert_eq!(error.reason, reason);
    assert_eq!(error.stage, "well-formed");
    error
}
fn accept(program: Program) {
    let expected = program.clone();
    let checked = WellFormedProgram::check(program).expect("valid program rejected");
    assert_eq!(checked.program(), &expected);
    assert_eq!(checked.clone(), checked);
}
fn actual(receipt: ReceiptReference<CardQuantityOutcome>) -> Expr<Nat> {
    Expr::Frozen(Box::new(NatFrozen::ReceiptActual {
        receipt: MeasuredReceipt::Cards { receipt },
    }))
}
fn signed(value: Expr<Nat>) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::FromNat { value }))
}
fn positive(value: Expr<Nat>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::CompareNat {
        left: value,
        comparison: Comparison::Greater,
        right: Expr::Literal(Nat(0)),
    }))
}
fn simultaneous(members: Vec<Located<Node>>, receipt: u64) -> Located<Node> {
    control(Control::Simultaneous {
        batch: InstructionBatch {
            members: members
                .into_iter()
                .map(|node| {
                    let Node::Instruction { instruction } = node.node else {
                        panic!("expected instruction")
                    };
                    Located {
                        source: node.source,
                        node: *instruction,
                    }
                })
                .collect(),
            membership: BatchMembership::AtBatchStart,
            choice_stages: vec![],
            commit: BatchCommit::SharedCapacityAtomic,
            event_views: BatchViews::BeforeAndAfter,
        },
        receipt: ReceiptId(receipt),
    })
}

#[test]
fn checked_body_is_owned_and_capabilities_are_not_a_certificate() {
    let mut original = bp08_body();
    let checked = WellFormedProgram::check(original.clone()).expect("BP08-038 signed live count");
    original.steps.clear();
    assert_eq!(checked.program().steps.len(), 1);
    assert!(Registry::current().supported().is_empty());
    let invalid = body(1, vec![draw(1, Expr::Bound(Binding::new(1, ScopeId(1))))]);
    let seed = card(invalid.clone(), "synthetic");
    Registry::test_support(&seed)
        .check_capabilities(&seed)
        .unwrap();
    reject(invalid, WellFormedReason::UnavailableBinding);
}

#[test]
fn declaration_must_dominate_a_use_of_the_exact_type_and_scope() {
    let binding = Binding::new(1, ScopeId(1));
    accept(body(
        1,
        vec![declare(1, binding), draw(2, Expr::Bound(binding))],
    ));
    reject(
        body(1, vec![draw(1, Expr::Bound(binding)), declare(2, binding)]),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            1,
            vec![
                declare(1, binding),
                draw(2, signed(Expr::Bound(Binding::new(1, ScopeId(1))))),
            ],
        ),
        WellFormedReason::BindingType,
    );
    reject(
        body(1, vec![declare(1, Binding::new(1, ScopeId(2)))]),
        WellFormedReason::WrongScope,
    );
    reject(
        body(1, vec![declare(1, binding), declare(2, binding)]),
        WellFormedReason::DuplicateBinding,
    );
}

#[test]
fn captures_require_a_definition_and_do_not_alias_regular_values() {
    let binding = Binding::new(1, ScopeId(1));
    let captured = Expr::Captured(CaptureRef { binding });
    accept(body(
        1,
        vec![
            capture(binding, Expr::Literal(Int(-2))),
            draw(1, captured.clone()),
        ],
    ));
    reject(
        body(1, vec![declare(1, binding), draw(2, captured)]),
        WellFormedReason::BindingType,
    );
    reject(
        body(
            1,
            vec![
                capture(binding, Expr::Literal(Int(1))),
                draw(1, Expr::Bound(binding)),
            ],
        ),
        WellFormedReason::BindingType,
    );
    reject(
        body(1, vec![capture(binding, Expr::Bound(binding))]),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            2,
            vec![capture(Binding::new(1, ScopeId(2)), Expr::Literal(Int(1)))],
        ),
        WellFormedReason::WrongScope,
    );
}

#[test]
fn every_branch_is_checked_without_implicit_join_or_sibling_visibility() {
    let binding = Binding::new(1, ScopeId(2));
    let branch = |else_body| {
        control(Control::If {
            condition: Expr::Literal(Bool(false)),
            then_body: body(2, vec![declare(1, binding)]),
            else_body,
        })
    };
    accept(body(
        1,
        vec![branch(Some(body(3, vec![draw(2, Expr::Literal(Int(1)))])))],
    ));
    reject(
        body(1, vec![branch(None), draw(2, Expr::Bound(binding))]),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            1,
            vec![branch(Some(body(3, vec![draw(2, Expr::Bound(binding))])))],
        ),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            1,
            vec![control(Control::If {
                condition: Expr::Literal(Bool(false)),
                then_body: body(2, vec![draw(1, Expr::Bound(binding))]),
                else_body: None,
            })],
        ),
        WellFormedReason::UnavailableBinding,
    );
}

#[test]
fn instruction_receipt_and_scope_identities_cannot_be_redeclared() {
    reject(
        body(
            1,
            vec![
                draw(1, Expr::Literal(Int(1))),
                draw(1, Expr::Literal(Int(2))),
            ],
        ),
        WellFormedReason::DuplicateInstruction,
    );
    let mut second = draw(2, Expr::Literal(Int(1)));
    if let Node::Instruction { instruction } = &mut second.node {
        instruction.receipt = ReceiptId(1);
    }
    reject(
        body(1, vec![draw(1, Expr::Literal(Int(1))), second]),
        WellFormedReason::DuplicateReceipt,
    );
    reject(
        body(
            1,
            vec![control(Control::Sequence {
                body: body(1, vec![]),
            })],
        ),
        WellFormedReason::DuplicateScope,
    );
    reject(
        body(
            1,
            vec![control(Control::If {
                condition: Expr::Literal(Bool(true)),
                then_body: body(2, vec![draw(1, Expr::Literal(Int(1)))]),
                else_body: Some(body(3, vec![draw(1, Expr::Literal(Int(1)))])),
            })],
        ),
        WellFormedReason::DuplicateInstruction,
    );
    accept(body(
        1,
        vec![control(Control::Sequence {
            body: body(2, vec![draw(1, Expr::Literal(Int(1)))]),
        })],
    ));
}

#[test]
fn receipts_check_producer_type_and_dominance() {
    let follow = draw(2, signed(actual(ReceiptReference::new(ReceiptId(1)))));
    accept(body(
        1,
        vec![draw(1, Expr::Literal(Int(1))), follow.clone()],
    ));
    reject(
        body(1, vec![follow.clone()]),
        WellFormedReason::UnavailableReceipt,
    );
    let look = instruction(
        Action::Look {
            viewer: PlayerRef::Controller {},
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(1)),
            mode: StepwiseMode::Exact,
        },
        1,
        "synthetic",
    );
    reject(
        body(1, vec![look, follow.clone()]),
        WellFormedReason::ReceiptType,
    );
    reject(
        body(
            1,
            vec![
                control(Control::Sequence {
                    body: body(2, vec![draw(1, Expr::Literal(Int(1)))]),
                }),
                follow,
            ],
        ),
        WellFormedReason::UnavailableReceipt,
    );
}

#[test]
fn prerequisites_are_checked_before_the_protected_instruction() {
    let guarded = |protects, predicate| {
        let mut node = draw(2, Expr::Literal(Int(1)));
        if let Node::Instruction { instruction } = &mut node.node {
            instruction.precondition = Some(Precondition {
                kind: PreconditionKind::CardTextPrerequisite,
                predicate,
                protects: InstructionId(protects),
                reason: PreconditionReason::CardText {},
            });
        }
        node
    };
    accept(body(
        1,
        vec![
            draw(1, Expr::Literal(Int(1))),
            guarded(2, positive(actual(ReceiptReference::new(ReceiptId(1))))),
        ],
    ));
    let error = reject(
        body(1, vec![guarded(9, Expr::Literal(Bool(true)))]),
        WellFormedReason::PreconditionTiming,
    );
    assert_eq!(error.ir_path, "/steps/0/instruction/precondition");
    assert_eq!(
        error.source.as_ref().map(|source| source.card_no.as_str()),
        Some("synthetic")
    );
    assert!(error.to_string().contains("PreconditionTiming"));
    reject(
        body(
            1,
            vec![guarded(
                2,
                positive(actual(ReceiptReference::new(ReceiptId(2)))),
            )],
        ),
        WellFormedReason::UnavailableReceipt,
    );
    let mut cost = draw(1, Expr::Literal(Int(1)));
    if let Node::Instruction { instruction } = &mut cost.node {
        instruction.context = InstructionContext::Cost;
    }
    reject(body(1, vec![cost]), WellFormedReason::CostOutsidePayment);
}

#[test]
fn batch_members_share_start_inputs_and_publish_outputs_together() {
    let result = Binding::new(1, ScopeId(1));
    let dice = instruction(
        Action::Dice {
            operation: DiceOperation::Roll {},
            result,
        },
        1,
        "synthetic",
    );
    accept(body(
        1,
        vec![
            simultaneous(vec![dice.clone(), draw(2, Expr::Literal(Int(1)))], 3),
            draw(4, signed(Expr::Bound(result))),
        ],
    ));
    for members in [
        vec![dice.clone(), draw(2, signed(Expr::Bound(result)))],
        vec![draw(2, signed(Expr::Bound(result))), dice],
    ] {
        reject(
            body(1, vec![simultaneous(members, 3)]),
            WellFormedReason::UnavailableBinding,
        );
    }
    let receipt_use = draw(2, signed(actual(ReceiptReference::new(ReceiptId(1)))));
    reject(
        body(
            1,
            vec![simultaneous(
                vec![draw(1, Expr::Literal(Int(1))), receipt_use.clone()],
                3,
            )],
        ),
        WellFormedReason::UnavailableReceipt,
    );
    accept(body(
        1,
        vec![
            simultaneous(vec![draw(1, Expr::Literal(Int(1)))], 3),
            receipt_use,
        ],
    ));
    reject(
        body(1, vec![simultaneous(vec![], 1)]),
        WellFormedReason::EmptyBatch,
    );
    reject(
        body(
            1,
            vec![simultaneous(vec![draw(1, Expr::Literal(Int(1)))], 1)],
        ),
        WellFormedReason::DuplicateReceipt,
    );
}

fn repeat(export: ReceiptExport, termination: Termination, check: IterationCheck) -> Located<Node> {
    control(Control::Repeat {
        body: RepeatBody::Program {
            body: body(3, vec![draw(1, Expr::Literal(Int(1)))]),
        },
        termination,
        check,
        scope: ScopeId(2),
        receipts: vec![export],
    })
}
fn card_export() -> ReceiptExport {
    ReceiptExport::CardQuantity {
        source: ReceiptReference::new(ReceiptId(1)),
        iteration: Binding::new(1, ScopeId(2)),
        collect: Some(Binding::new(2, ScopeId(1))),
    }
}
fn collect_sum() -> Expr<Nat> {
    Expr::Compute(Box::new(NatOperation::ReceiptActualSum {
        receipts: MeasuredReceiptList::Cards {
            receipts: Binding::new(2, ScopeId(1)),
        },
    }))
}

#[test]
fn repeat_exports_are_visible_only_at_their_declared_phase() {
    let until = Termination::Until {
        predicate: positive(actual(ReceiptReference::iteration(Binding::new(
            1,
            ScopeId(2),
        )))),
    };
    accept(body(
        1,
        vec![
            repeat(card_export(), until.clone(), IterationCheck::AfterIteration),
            draw(2, signed(collect_sum())),
        ],
    ));
    reject(
        body(
            1,
            vec![repeat(
                card_export(),
                until,
                IterationCheck::BeforeIteration,
            )],
        ),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            1,
            vec![repeat(
                card_export(),
                Termination::Until {
                    predicate: positive(collect_sum()),
                },
                IterationCheck::AfterIteration,
            )],
        ),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            1,
            vec![
                repeat(
                    card_export(),
                    Termination::Count {
                        count: Expr::Literal(Nat(1)),
                    },
                    IterationCheck::BeforeIteration,
                ),
                draw(
                    2,
                    signed(actual(ReceiptReference::iteration(Binding::new(
                        1,
                        ScopeId(2),
                    )))),
                ),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            1,
            vec![
                draw(2, signed(collect_sum())),
                repeat(
                    card_export(),
                    Termination::Count {
                        count: Expr::Literal(Nat(1)),
                    },
                    IterationCheck::BeforeIteration,
                ),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
}

#[test]
fn repeat_rejects_wrong_producers_outer_receipts_and_nonlocal_exports() {
    let count = Termination::Count {
        count: Expr::Literal(Nat(2)),
    };
    reject(
        body(
            1,
            vec![repeat(
                ReceiptExport::Look {
                    source: ReceiptReference::new(ReceiptId(1)),
                    iteration: Binding::new(1, ScopeId(2)),
                    collect: None,
                },
                count.clone(),
                IterationCheck::BeforeIteration,
            )],
        ),
        WellFormedReason::ReceiptType,
    );
    reject(
        body(
            1,
            vec![
                draw(2, Expr::Literal(Int(1))),
                repeat(
                    ReceiptExport::CardQuantity {
                        source: ReceiptReference::new(ReceiptId(2)),
                        iteration: Binding::new(1, ScopeId(2)),
                        collect: None,
                    },
                    count.clone(),
                    IterationCheck::BeforeIteration,
                ),
            ],
        ),
        WellFormedReason::InvalidExport,
    );
    reject(
        body(
            1,
            vec![repeat(
                ReceiptExport::CardQuantity {
                    source: ReceiptReference::iteration(Binding::new(1, ScopeId(2))),
                    iteration: Binding::new(2, ScopeId(2)),
                    collect: None,
                },
                count.clone(),
                IterationCheck::BeforeIteration,
            )],
        ),
        WellFormedReason::InvalidExport,
    );
    reject(
        body(
            1,
            vec![repeat(
                ReceiptExport::CardQuantity {
                    source: ReceiptReference::new(ReceiptId(1)),
                    iteration: Binding::new(1, ScopeId(2)),
                    collect: Some(Binding::new(2, ScopeId(2))),
                },
                count,
                IterationCheck::BeforeIteration,
            )],
        ),
        WellFormedReason::WrongScope,
    );
}

#[test]
fn candidates_and_each_object_bindings_do_not_escape_their_body() {
    let binding = Binding::new(1, ScopeId(2));
    let input = Selector::Zone {
        players: controller_set(),
        zone: ZoneKind::Deck,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::MemberOf {
            object: ObjectOperand::Bound { binding },
            set: Selector::Objects { objects: vec![] },
        }))),
    };
    let reveal = |selector| instruction(Action::Reveal { objects: selector }, 1, "synthetic");
    accept(body(1, vec![reveal(input.clone())]));
    reject(
        body(
            1,
            vec![
                reveal(input),
                instruction(
                    Action::Posture {
                        objects: Selector::Objects {
                            objects: vec![ObjectOperand::Bound { binding }],
                        },
                        posture: Posture::Acted,
                    },
                    2,
                    "synthetic",
                ),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
    let each = control(Control::EachObject {
        input: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        binding,
        body: body(
            2,
            vec![reveal(Selector::Objects {
                objects: vec![ObjectOperand::Bound { binding }],
            })],
        ),
        order: None,
    });
    accept(body(1, vec![each.clone()]));
    reject(
        body(
            1,
            vec![each, draw(2, Expr::Bound(Binding::new(1, ScopeId(2))))],
        ),
        WellFormedReason::UnavailableBinding,
    );
}

#[test]
fn each_player_carries_same_player_outputs_across_stages() {
    let player = Binding::new(1, ScopeId(2));
    let declaration = Binding::new(2, ScopeId(2));
    let each = |count| {
        control(Control::EachPlayer {
            players: controller_set(),
            player,
            choice_order: PlayerOrder::TurnPlayerFirst,
            stages: vec![
                PlayerStage {
                    body: body(2, vec![declare(1, declaration)]),
                    mode: StageMode::ChoiceProcedure {},
                },
                PlayerStage {
                    body: body(
                        3,
                        vec![with_instruction(
                            instruction(
                                Action::Draw {
                                    player: PlayerRef::Bound { binding: player },
                                    count,
                                    mode: StepwiseMode::Exact,
                                },
                                2,
                                "synthetic",
                            ),
                            |instruction| instruction.actor = PlayerRef::Bound { binding: player },
                        )],
                    ),
                    mode: StageMode::NonChoiceSimultaneous {},
                },
            ],
        })
    };
    accept(body(1, vec![each(Expr::Literal(Int(1)))]));
    accept(body(1, vec![each(Expr::Bound(declaration))]));
    reject(
        body(
            1,
            vec![
                each(Expr::Literal(Int(1))),
                instruction(
                    Action::Draw {
                        player: PlayerRef::Bound { binding: player },
                        count: Expr::Literal(Int(1)),
                        mode: StepwiseMode::Exact,
                    },
                    3,
                    "synthetic",
                ),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
}

#[test]
fn unchecked_connections_fail_closed_instead_of_minting_a_certificate() {
    for seed in [cp04(true, false), cp04(false, false)] {
        let error = WellFormedCardProgram::check(seed.node).unwrap_err();
        assert_eq!(error.reason, WellFormedReason::UnsupportedShape);
        assert!(error.ir_path.ends_with("/abilities/0"), "{}", error.ir_path);
    }
    reject(
        body(
            1,
            vec![control(Control::Pay {
                at: PaymentTime::ResolutionChoice,
                costs: CostSpec::Alternative {
                    alternatives: vec![],
                },
                optional: PaymentOptionality::Required,
                on_paid: body(2, vec![]),
                receipt: ReceiptId(1),
            })],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn arithmetic_checks_all_operands_and_requires_a_proven_divisor() {
    let math = |operation, arguments| {
        Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation,
            arguments,
        }))
    };
    for operation in [
        Arithmetic::Add,
        Arithmetic::Subtract,
        Arithmetic::Multiply,
        Arithmetic::FloorDivide,
    ] {
        accept(body(
            1,
            vec![draw(
                1,
                math(
                    operation,
                    vec![Expr::Literal(Int(-3)), Expr::Literal(Int(2))],
                ),
            )],
        ));
        reject(
            body(
                1,
                vec![draw(1, math(operation, vec![Expr::Literal(Int(1))]))],
            ),
            WellFormedReason::ArithmeticArity,
        );
    }
    accept(body(
        1,
        vec![draw(
            1,
            math(Arithmetic::HalfUp, vec![Expr::Literal(Int(3))]),
        )],
    ));
    reject(
        body(
            1,
            vec![draw(
                1,
                math(
                    Arithmetic::FloorDivide,
                    vec![Expr::Literal(Int(1)), Expr::Literal(Int(0))],
                ),
            )],
        ),
        WellFormedReason::InvalidDivisor,
    );
    reject(
        body(
            1,
            vec![draw(
                1,
                math(
                    Arithmetic::FloorDivide,
                    vec![
                        Expr::Literal(Int(1)),
                        Expr::Live(Box::new(IntRead::PlayerResource {
                            player: PlayerRef::Controller {},
                            property: PlayerResource::Life,
                        })),
                    ],
                ),
            )],
        ),
        WellFormedReason::UnsupportedShape,
    );
    let conditional = Expr::Compute(Box::new(IntOperation::Conditional {
        condition: Expr::Literal(Bool(true)),
        then_value: Expr::Literal(Int(2)),
        else_value: Expr::Bound(Binding::new(1, ScopeId(1))),
    }));
    reject(
        body(1, vec![draw(1, conditional)]),
        WellFormedReason::UnavailableBinding,
    );
}

#[test]
fn variadic_extrema_and_unary_half_up_check_their_operand_counts() {
    let math = |operation, arguments| {
        Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation,
            arguments,
        }))
    };
    for operation in [Arithmetic::Minimum, Arithmetic::Maximum] {
        for arguments in [
            vec![Expr::Literal(Int(-3))],
            vec![
                Expr::Literal(Int(-3)),
                Expr::Literal(Int(2)),
                Expr::Literal(Int(5)),
            ],
        ] {
            accept(body(1, vec![draw(1, math(operation, arguments))]));
        }
        reject(
            body(1, vec![draw(1, math(operation, vec![]))]),
            WellFormedReason::ArithmeticArity,
        );
        reject(
            body(
                1,
                vec![draw(
                    1,
                    math(
                        operation,
                        vec![
                            Expr::Literal(Int(1)),
                            Expr::Literal(Int(2)),
                            Expr::Bound(Binding::new(1, ScopeId(1))),
                        ],
                    ),
                )],
            ),
            WellFormedReason::UnavailableBinding,
        );
    }
    reject(
        body(
            1,
            vec![draw(
                1,
                math(
                    Arithmetic::HalfUp,
                    vec![Expr::Literal(Int(3)), Expr::Literal(Int(1))],
                ),
            )],
        ),
        WellFormedReason::ArithmeticArity,
    );
}

#[test]
fn choice_free_batches_and_stages_cannot_hide_a_declaration() {
    reject(
        body(
            1,
            vec![simultaneous(
                vec![declare(1, Binding::new(1, ScopeId(1)))],
                2,
            )],
        ),
        WellFormedReason::UnsupportedShape,
    );
    reject(
        body(
            1,
            vec![control(Control::EachPlayer {
                players: controller_set(),
                player: Binding::new(1, ScopeId(2)),
                choice_order: PlayerOrder::TurnPlayerFirst,
                stages: vec![PlayerStage {
                    body: body(
                        2,
                        vec![control(Control::Sequence {
                            body: body(3, vec![declare(1, Binding::new(2, ScopeId(3)))]),
                        })],
                    ),
                    mode: StageMode::NonChoiceSimultaneous {},
                }],
            })],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn collected_nonmovement_members_keep_their_producer_type() {
    let look = instruction(
        Action::Look {
            viewer: PlayerRef::Owner {},
            zone: ZoneRef {
                player: PlayerRef::Owner {},
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(2)),
            mode: StepwiseMode::Exact,
        },
        1,
        "synthetic",
    );
    let collected = Binding::new(2, ScopeId(1));
    let loop_node = control(Control::Repeat {
        body: RepeatBody::Program {
            body: body(3, vec![look]),
        },
        termination: Termination::While {
            predicate: Expr::Literal(Bool(false)),
        },
        check: IterationCheck::BeforeIteration,
        scope: ScopeId(2),
        receipts: vec![ReceiptExport::Look {
            source: ReceiptReference::new(ReceiptId(1)),
            iteration: Binding::new(1, ScopeId(2)),
            collect: Some(collected),
        }],
    });
    let reveal = |receipts| {
        instruction(
            Action::Reveal {
                objects: Selector::CollectedMembers {
                    receipts,
                    projection: MemberProjection::Before,
                    deduplicate: GenerationDeduplication::ByObjectAndGeneration,
                },
            },
            2,
            "synthetic",
        )
    };
    accept(body(
        1,
        vec![
            loop_node.clone(),
            reveal(ObjectReceiptList::Look {
                receipts: collected,
            }),
        ],
    ));
    reject(
        body(
            1,
            vec![
                loop_node,
                reveal(ObjectReceiptList::Flip {
                    receipts: Binding::new(2, ScopeId(1)),
                }),
            ],
        ),
        WellFormedReason::BindingType,
    );
}

fn look(id: u64, count: Expr<Nat>, mode: StepwiseMode) -> Located<Node> {
    instruction(
        Action::Look {
            viewer: PlayerRef::Controller {},
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            count,
            mode,
        },
        id,
        "synthetic",
    )
}
fn stepwise(id: u64, timing: StepwiseTiming) -> StepwiseMode {
    StepwiseMode::UpToStepwise {
        decision: Box::new(StepwiseDecision {
            id: DecisionId(id),
            actor: PlayerRef::Controller {},
            visibility: Visibility::Only {
                players: controller_set(),
            },
            timing,
        }),
    }
}
fn up_to_draw(id: u64) -> Located<Node> {
    instruction(
        Action::Draw {
            player: PlayerRef::Controller {},
            count: Expr::Literal(Int(2)),
            mode: stepwise(id, StepwiseTiming::BeforeDraw),
        },
        id,
        "synthetic",
    )
}
fn each_player(stage: Located<Node>, mode: StageMode) -> Located<Node> {
    control(Control::EachPlayer {
        players: controller_set(),
        player: Binding::new(1, ScopeId(2)),
        choice_order: PlayerOrder::TurnPlayerFirst,
        stages: vec![PlayerStage {
            body: body(3, vec![stage]),
            mode,
        }],
    })
}

#[test]
fn choice_free_batches_and_stages_cannot_hide_stepwise_decisions() {
    accept(body(1, vec![up_to_draw(1)]));
    accept(body(
        1,
        vec![each_player(
            look(
                1,
                Expr::Literal(Nat(3)),
                stepwise(9, StepwiseTiming::AfterLook),
            ),
            StageMode::ChoiceProcedure {},
        )],
    ));
    accept(body(
        1,
        vec![simultaneous(
            vec![
                draw(1, Expr::Literal(Int(1))),
                look(2, Expr::Literal(Nat(1)), StepwiseMode::Exact),
            ],
            3,
        )],
    ));
    for members in [
        vec![up_to_draw(1)],
        vec![
            draw(1, Expr::Literal(Int(1))),
            look(
                2,
                Expr::Literal(Nat(1)),
                stepwise(9, StepwiseTiming::AfterLook),
            ),
        ],
    ] {
        reject(
            body(1, vec![simultaneous(members, 9)]),
            WellFormedReason::UnsupportedShape,
        );
    }
    for stage in [
        up_to_draw(1),
        look(
            1,
            Expr::Literal(Nat(3)),
            stepwise(9, StepwiseTiming::AfterLook),
        ),
        control(Control::If {
            condition: Expr::Literal(Bool(true)),
            then_body: body(4, vec![up_to_draw(1)]),
            else_body: None,
        }),
    ] {
        reject(
            body(
                1,
                vec![each_player(stage, StageMode::NonChoiceSimultaneous {})],
            ),
            WellFormedReason::UnsupportedShape,
        );
    }
}

#[test]
fn resolution_captures_are_the_root_entry_prefix_and_independent() {
    let natural = |id, at, value| Located {
        source: source("synthetic"),
        node: Node::Capture {
            capture: Capture::Natural {
                at,
                value,
                into: CaptureRef {
                    binding: Binding::new(id, ScopeId(1)),
                },
            },
        },
    };
    let entry = CaptureTime::AtResolution { scope: ScopeId(1) };
    let deck = || {
        Expr::Live(Box::new(NatRead::ZoneCount {
            selector: Selector::Zone {
                players: controller_set(),
                zone: ZoneKind::Deck,
                view: SelectionView::Current {},
                binding: Binding::new(9, ScopeId(1)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        }))
    };
    let frozen = |id| {
        Expr::Captured(CaptureRef {
            binding: Binding::new(id, ScopeId(1)),
        })
    };
    accept(body(
        1,
        vec![
            natural(1, entry, deck()),
            natural(2, entry, Expr::Literal(Nat(2))),
            draw(1, Expr::Literal(Int(1))),
            draw(2, signed(frozen(1))),
        ],
    ));
    // A capture after the body changed state would freeze the changed view.
    reject(
        body(
            1,
            vec![draw(1, Expr::Literal(Int(1))), natural(1, entry, deck())],
        ),
        WellFormedReason::UnsupportedShape,
    );
    reject(
        body(
            1,
            vec![
                natural(1, entry, Expr::Literal(Nat(1))),
                natural(2, entry, frozen(1)),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            1,
            vec![
                natural(1, entry, actual(ReceiptReference::new(ReceiptId(1)))),
                draw(1, Expr::Literal(Int(1))),
            ],
        ),
        WellFormedReason::UnavailableReceipt,
    );
    let nested_capture = |scope| Located {
        source: source("synthetic"),
        node: Node::Capture {
            capture: Capture::Natural {
                at: CaptureTime::AtResolution {
                    scope: ScopeId(scope),
                },
                value: Expr::Literal(Nat(1)),
                into: CaptureRef {
                    binding: Binding::new(1, ScopeId(2)),
                },
            },
        },
    };
    for nested in [
        control(Control::Sequence {
            body: body(2, vec![nested_capture(1)]),
        }),
        control(Control::If {
            condition: Expr::Literal(Bool(true)),
            then_body: body(2, vec![nested_capture(1)]),
            else_body: None,
        }),
    ] {
        reject(body(1, vec![nested]), WellFormedReason::UnsupportedShape);
    }
    reject(
        body(
            1,
            vec![control(Control::Sequence {
                body: body(2, vec![nested_capture(2)]),
            })],
        ),
        WellFormedReason::WrongScope,
    );
}

fn guard(condition: Expr<Bool>) -> Located<Node> {
    control(Control::If {
        condition,
        then_body: body(2, vec![]),
        else_body: None,
    })
}
fn reveal_objects(objects: Selector) -> Located<Node> {
    instruction(Action::Reveal { objects }, 1, "synthetic")
}
fn opponents_of(relative_to: PlayerRef) -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::Opponents,
        relative_to,
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::Implicit,
    }
}
fn counter(holder: Holder, operation: CounterOperation, amount: Expr<Nat>) -> Located<Node> {
    instruction(
        Action::Counter {
            holder,
            kind: CounterKind("stack".to_owned()),
            operation,
            amount,
        },
        1,
        "synthetic",
    )
}
fn with_instruction(mut node: Located<Node>, edit: impl FnOnce(&mut Instruction)) -> Located<Node> {
    if let Node::Instruction { instruction } = &mut node.node {
        edit(instruction);
    }
    node
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One table keeps every accepted operand position beside its dominance check."
)]
fn every_accepted_operand_position_requires_a_dominating_definition() {
    let player = || PlayerRef::Bound {
        binding: Binding::new(91, ScopeId(1)),
    };
    let object = || ObjectOperand::Bound {
        binding: Binding::new(92, ScopeId(1)),
    };
    let objects = || Selector::Bound {
        reference: Binding::new(93, ScopeId(1)),
    };
    let int = || Expr::<Int>::Bound(Binding::new(94, ScopeId(1)));
    let nat = || Expr::<Nat>::Bound(Binding::new(95, ScopeId(1)));
    let truth = || Expr::<Bool>::Bound(Binding::new(96, ScopeId(1)));
    let none = || Selector::Objects { objects: vec![] };
    let candidate = || Binding::new(1, ScopeId(3));
    let yes = || Expr::Literal(Bool(true));
    let one = || Expr::Literal(Nat(1));
    let int_op = |operation| Expr::Compute(Box::new(operation));
    let nat_op = |operation| Expr::Compute(Box::new(operation));
    let nat_live = |read| Expr::Live(Box::new(read));
    let predicate = |predicate| Expr::Compute(Box::new(predicate));
    let undefined = vec![
        draw(
            1,
            Expr::Live(Box::new(IntRead::PlayerResource {
                player: player(),
                property: PlayerResource::Life,
            })),
        ),
        draw(
            1,
            int_op(IntOperation::Sum {
                objects: objects(),
                property: NumericProperty::Cost,
            }),
        ),
        draw(
            1,
            int_op(IntOperation::Conditional {
                condition: truth(),
                then_value: Expr::Literal(Int(1)),
                else_value: Expr::Literal(Int(0)),
            }),
        ),
        draw(1, signed(nat())),
        look(
            1,
            nat_live(NatRead::ZoneCount {
                selector: objects(),
            }),
            StepwiseMode::Exact,
        ),
        look(
            1,
            nat_live(NatRead::Counter {
                holder: Holder::Object { object: object() },
                kind: CounterKind("stack".to_owned()),
            }),
            StepwiseMode::Exact,
        ),
        look(
            1,
            nat_live(NatRead::ZoneCapacity {
                player: player(),
                zone: CapacityZone::Field,
                property: CapacityProperty::Free,
            }),
            StepwiseMode::Exact,
        ),
        look(
            1,
            nat_live(NatRead::TurnOrdinal { player: player() }),
            StepwiseMode::Exact,
        ),
        look(
            1,
            nat_op(NatOperation::CheckedInt { value: int() }),
            StepwiseMode::Exact,
        ),
        look(
            1,
            nat_op(NatOperation::Count { objects: objects() }),
            StepwiseMode::Exact,
        ),
        look(
            1,
            nat_op(NatOperation::DistinctCount {
                objects: objects(),
                property: ObjectProperty::Names,
            }),
            StepwiseMode::Exact,
        ),
        look(
            1,
            nat_op(NatOperation::Conditional {
                condition: truth(),
                then_value: one(),
                else_value: one(),
            }),
            StepwiseMode::Exact,
        ),
        look(
            1,
            nat_op(NatOperation::ReceiptActualSum {
                receipts: MeasuredReceiptList::Damage {
                    receipts: Binding::new(97, ScopeId(1)),
                },
            }),
            StepwiseMode::Exact,
        ),
        look(
            1,
            nat_op(NatOperation::ReceiptActualSum {
                receipts: MeasuredReceiptList::Counters {
                    receipts: Binding::new(97, ScopeId(1)),
                },
            }),
            StepwiseMode::Exact,
        ),
        guard(Expr::Live(Box::new(BoolRead::Awakening {
            player: player(),
        }))),
        guard(Expr::Live(Box::new(BoolRead::OwnTurn { player: player() }))),
        guard(Expr::Live(Box::new(BoolRead::State {
            object: ObjectView::Current {
                object: Box::new(object()),
            },
            property: BooleanState::FaceUp,
        }))),
        guard(predicate(Predicate::And {
            predicates: vec![yes(), truth()],
        })),
        guard(predicate(Predicate::Or {
            predicates: vec![yes(), truth()],
        })),
        guard(predicate(Predicate::Not { predicate: truth() })),
        guard(predicate(Predicate::CompareInt {
            left: Expr::Literal(Int(0)),
            comparison: Comparison::Equal,
            right: int(),
        })),
        guard(predicate(Predicate::CompareNat {
            left: one(),
            comparison: Comparison::Equal,
            right: nat(),
        })),
        guard(predicate(Predicate::Exists { objects: objects() })),
        guard(predicate(Predicate::MemberOf {
            object: object(),
            set: none(),
        })),
        guard(predicate(Predicate::MemberOf {
            object: ObjectOperand::SourceHolder {},
            set: objects(),
        })),
        guard(predicate(Predicate::All {
            objects: objects(),
            binding: candidate(),
            predicate: yes(),
        })),
        guard(predicate(Predicate::Any {
            objects: none(),
            binding: candidate(),
            predicate: truth(),
        })),
        reveal_objects(Selector::Objects {
            objects: vec![object()],
        }),
        reveal_objects(Selector::Zone {
            players: opponents_of(player()),
            zone: ZoneKind::Hand,
            view: SelectionView::Current {},
            binding: candidate(),
            predicate: Box::new(yes()),
        }),
        reveal_objects(Selector::Filter {
            input: Box::new(objects()),
            binding: candidate(),
            predicate: Box::new(yes()),
        }),
        reveal_objects(Selector::Filter {
            input: Box::new(none()),
            binding: candidate(),
            predicate: Box::new(truth()),
        }),
        reveal_objects(Selector::Union {
            sets: vec![none(), objects()],
        }),
        reveal_objects(Selector::Intersection {
            sets: vec![objects()],
        }),
        reveal_objects(Selector::Difference {
            input: Box::new(none()),
            except: Box::new(objects()),
        }),
        reveal_objects(Selector::Ordered {
            zone: ZoneRef {
                player: player(),
                kind: ZoneKind::Deck,
            },
            start: Position::Top {},
            count: one(),
            direction: SliceDirection::Forward,
        }),
        reveal_objects(Selector::Ordered {
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            start: Position::Top {},
            count: nat(),
            direction: SliceDirection::Forward,
        }),
        reveal_objects(Selector::Captured {
            reference: CaptureRef {
                binding: Binding::new(98, ScopeId(1)),
            },
        }),
        instruction(
            Action::Draw {
                player: PlayerRef::RequireOne {
                    players: Box::new(opponents_of(player())),
                },
                count: Expr::Literal(Int(1)),
                mode: StepwiseMode::Exact,
            },
            1,
            "synthetic",
        ),
        with_instruction(look(1, one(), StepwiseMode::Exact), |instruction| {
            instruction.action = Action::Look {
                viewer: player(),
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Deck,
                },
                count: Expr::Literal(Nat(1)),
                mode: StepwiseMode::Exact,
            };
        }),
        with_instruction(look(1, one(), StepwiseMode::Exact), |instruction| {
            instruction.action = Action::Look {
                viewer: PlayerRef::Controller {},
                zone: ZoneRef {
                    player: player(),
                    kind: ZoneKind::Deck,
                },
                count: Expr::Literal(Nat(1)),
                mode: StepwiseMode::Exact,
            };
        }),
        counter(
            Holder::Object { object: object() },
            CounterOperation::Add {},
            one(),
        ),
        counter(
            Holder::Player {
                player: PlayerRef::Controller {},
            },
            CounterOperation::Move {
                to: Holder::Player { player: player() },
            },
            one(),
        ),
        counter(
            Holder::Player {
                player: PlayerRef::Controller {},
            },
            CounterOperation::Remove {},
            nat(),
        ),
        with_instruction(draw(1, Expr::Literal(Int(1))), |instruction| {
            instruction.action = Action::Declare {
                actor: player(),
                declaration: Declaration::NonnegativeInteger {
                    binding: Binding::new(1, ScopeId(1)),
                },
            };
        }),
        with_instruction(draw(1, Expr::Literal(Int(1))), |instruction| {
            instruction.actor = player();
        }),
        with_instruction(draw(1, Expr::Literal(Int(1))), |instruction| {
            instruction.source.holder = object();
        }),
        with_instruction(draw(1, Expr::Literal(Int(1))), |instruction| {
            instruction.source.controller = player();
        }),
        control(Control::EachObject {
            input: objects(),
            binding: Binding::new(1, ScopeId(2)),
            body: body(2, vec![]),
            order: None,
        }),
        control(Control::EachPlayer {
            players: opponents_of(player()),
            player: Binding::new(1, ScopeId(2)),
            choice_order: PlayerOrder::TurnPlayerFirst,
            stages: vec![PlayerStage {
                body: body(3, vec![]),
                mode: StageMode::ChoiceProcedure {},
            }],
        }),
        control(Control::Repeat {
            body: RepeatBody::Program {
                body: body(3, vec![]),
            },
            termination: Termination::Count { count: nat() },
            check: IterationCheck::BeforeIteration,
            scope: ScopeId(2),
            receipts: vec![],
        }),
    ];
    for (index, step) in undefined.into_iter().enumerate() {
        let error = WellFormedProgram::check(body(1, vec![step]))
            .expect_err("undefined operand passed validation");
        assert_eq!(
            error.reason,
            WellFormedReason::UnavailableBinding,
            "case {index}"
        );
    }
    for selector in [
        Selector::Created {
            receipt: missing_receipt(),
        },
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Look {
                receipt: missing_receipt(),
            },
            projection: MemberProjection::After,
        },
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Flip {
                receipt: missing_receipt(),
            },
            projection: MemberProjection::After,
        },
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Token {
                receipt: missing_receipt(),
            },
            projection: MemberProjection::After,
        },
        Selector::ReceiptMembers {
            receipt: ObjectReceipt::Batch {
                receipt: missing_receipt(),
            },
            projection: MemberProjection::After,
        },
    ] {
        reject(
            body(1, vec![reveal_objects(selector)]),
            WellFormedReason::UnavailableReceipt,
        );
    }
}

#[test]
fn defined_operands_pass_through_accepted_value_positions() {
    let element = Binding::new(1, ScopeId(2));
    let here = || ObjectOperand::Bound { binding: element };
    let candidate = Binding::new(2, ScopeId(2));
    let flipped = instruction(
        Action::FlipDown {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
        1,
        "synthetic",
    );
    let members = |receipt| Selector::ReceiptMembers {
        receipt,
        projection: MemberProjection::After,
    };
    let inside = vec![with_instruction(
        counter(
            Holder::Object { object: here() },
            CounterOperation::Move {
                to: Holder::Player {
                    player: PlayerRef::RequireOne {
                        players: Box::new(opponents_of(PlayerRef::Controller {})),
                    },
                },
            },
            Expr::Live(Box::new(NatRead::Counter {
                holder: Holder::Object { object: here() },
                kind: CounterKind("stack".to_owned()),
            })),
        ),
        |instruction| {
            instruction.id = InstructionId(5);
            instruction.receipt = ReceiptId(5);
            instruction.precondition = Some(Precondition {
                kind: PreconditionKind::CardTextPrerequisite,
                predicate: Expr::Compute(Box::new(Predicate::And {
                    predicates: vec![
                        Expr::Live(Box::new(BoolRead::State {
                            object: ObjectView::Current {
                                object: Box::new(here()),
                            },
                            property: BooleanState::FaceUp,
                        })),
                        Expr::Compute(Box::new(Predicate::Any {
                            objects: members(ObjectReceipt::Flip {
                                receipt: ReceiptReference::new(ReceiptId(1)),
                            }),
                            binding: candidate,
                            predicate: Expr::Compute(Box::new(Predicate::MemberOf {
                                object: ObjectOperand::Bound { binding: candidate },
                                set: Selector::Objects {
                                    objects: vec![here()],
                                },
                            })),
                        })),
                    ],
                })),
                protects: InstructionId(5),
                reason: PreconditionReason::CardText {},
            });
        },
    )];
    accept(body(
        1,
        vec![
            flipped,
            simultaneous(vec![look(2, Expr::Literal(Nat(2)), StepwiseMode::Exact)], 3),
            control(Control::EachObject {
                input: Selector::Difference {
                    input: Box::new(Selector::Union {
                        sets: vec![
                            members(ObjectReceipt::Look {
                                receipt: ReceiptReference::new(ReceiptId(2)),
                            }),
                            members(ObjectReceipt::Batch {
                                receipt: ReceiptReference::new(ReceiptId(3)),
                            }),
                        ],
                    }),
                    except: Box::new(Selector::Intersection {
                        sets: vec![Selector::Ordered {
                            zone: ZoneRef {
                                player: PlayerRef::Controller {},
                                kind: ZoneKind::Deck,
                            },
                            start: Position::Top {},
                            count: Expr::Literal(Nat(1)),
                            direction: SliceDirection::Forward,
                        }],
                    }),
                },
                binding: element,
                body: body(2, inside),
                order: Some(IterationOrder::Printed),
            }),
        ],
    ));
}

fn missing_receipt<T>() -> ReceiptReference<T> {
    ReceiptReference::new(ReceiptId(9))
}

mod decisions;
