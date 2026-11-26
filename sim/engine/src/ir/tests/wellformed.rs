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
fn checked_body_owns_its_ir_and_checks_bindings() {
    let mut original = bp08_body();
    let checked = WellFormedProgram::check(original.clone()).expect("BP08-038 signed live count");
    original.steps.clear();
    assert_eq!(checked.program().steps.len(), 1);
    let invalid = body(1, vec![draw(1, Expr::Bound(Binding::new(1, ScopeId(1))))]);
    reject(invalid, WellFormedReason::UnavailableBinding);
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
fn unchecked_connections_fail_closed_instead_of_minting_a_certificate() {
    for seed in [cp04(true, false), cp04(false, false)] {
        assert!(
            WellFormedCardProgram::check(seed.node).is_err(),
            "UB qualification does not repair disconnected source and trigger instructions"
        );
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
            purpose: LookPurpose::Private,
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
                exchange: None,
                additional: None,
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

#[test]
fn allocated_transform_owns_its_decision_and_bounds_the_selection() {
    let allocated = |objects: usize, decision: u64| Action::Transform {
        objects: Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}; objects],
        },
        into: TokenAllocation::EachExactlyOnce {
            templates: vec![
                TokenTemplateId("first".into()),
                TokenTemplateId("second".into()),
            ],
            total: Expr::Literal(Nat(2)),
            counts: Binding::new(99, ScopeId(1)),
            actor: PlayerRef::Controller {},
            decision: DecisionId(decision),
        },
        entry_effects: vec![],
    };
    accept(body(1, vec![instruction(allocated(1, 1), 1, "synthetic")]));
    reject(
        body(
            1,
            vec![
                instruction(allocated(1, 1), 1, "synthetic"),
                instruction(allocated(1, 1), 2, "synthetic"),
            ],
        ),
        WellFormedReason::DuplicateDecision,
    );
    // Three objects can never each receive a different one of two templates.
    reject(
        body(1, vec![instruction(allocated(3, 1), 1, "synthetic")]),
        WellFormedReason::UnsupportedShape,
    );
}

mod history_plans;
