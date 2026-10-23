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
fn with_instruction(mut node: Located<Node>, edit: impl FnOnce(&mut Instruction)) -> Located<Node> {
    if let Node::Instruction { instruction } = &mut node.node {
        edit(instruction);
    }
    node
}

#[test]
fn operands_require_a_dominating_definition() {
    for node in [
        draw(1, Expr::Bound(Binding::new(94, ScopeId(1)))),
        reveal_objects(Selector::Bound {
            reference: Binding::new(93, ScopeId(1)),
        }),
        guard(Expr::Compute(Box::new(Predicate::Not {
            predicate: Expr::Bound(Binding::new(96, ScopeId(1))),
        }))),
    ] {
        reject(body(1, vec![node]), WellFormedReason::UnavailableBinding);
    }
}

mod decisions;

#[test]
fn card_play_costs_check_their_own_entry_and_keep_rejection_sources() {
    let change = |amount| Located {
        source: source("synthetic"),
        node: PlayCostModifier {
            condition: None,
            change: CostChange::Set {
                amount: Expr::Literal(Int(amount)),
            },
        },
    };
    let mut seed = CardProgram {
        abilities: vec![],
        card_play_costs: vec![Located {
            source: source("synthetic"),
            node: FacePlayCost {
                face: FaceId("base".into()),
                modifiers: vec![change(7)],
            },
        }],
    };
    WellFormedCardProgram::check(seed.clone()).unwrap();
    let mut second = change(3);
    second.source.author_path = "/play/cost_set/1".into();
    second.node.condition = Some(Expr::Literal(Bool(false)));
    seed.card_play_costs[0].node.modifiers.push(second.clone());
    let failure = WellFormedCardProgram::check(seed.clone()).unwrap_err();
    assert_eq!(failure.source.as_deref(), Some(&second.source));
    seed.card_play_costs[0].node.modifiers.pop();
    seed.card_play_costs[0].node.modifiers[0].node.condition =
        Some(Expr::Frozen(Box::new(BoolFrozen::PaymentComplete {
            receipt: PaymentReference::Local {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
        })));
    let receipt_failure = WellFormedCardProgram::check(seed).unwrap_err();
    assert_eq!(receipt_failure.reason, WellFormedReason::UnavailableReceipt);
    assert!(
        receipt_failure
            .ir_path
            .contains("card_play_costs/0/modifiers/0/condition")
    );
}
