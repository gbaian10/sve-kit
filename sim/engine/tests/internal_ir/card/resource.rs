use super::*;
use sve_engine::game::internal::ResourceOutcome;

fn recovery(
    id: u64,
    player: PlayerRef,
    resource: Resource,
    operation: ResourceOperation,
    amount: i64,
) -> Located<Node> {
    instruction(
        id,
        Action::Resource {
            player,
            resource,
            operation,
            amount: Expr::Literal(Int(amount)),
        },
    )
}

#[test]
fn pp_recovery_retains_actual_capped_results_without_payment_completion() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = setup(seat, json!([]));
        initial["players"][actor]["pp"] = json!({"current":3,"max":5});
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "pp-recovery",
        )
        .unwrap();
        let result = game
            .execute_internal(
                &checked(vec![
                    recovery(
                        1,
                        PlayerRef::Controller {},
                        Resource::PpCurrent,
                        ResourceOperation::Add,
                        3,
                    ),
                    recovery(
                        2,
                        PlayerRef::Controller {},
                        Resource::PpCurrent,
                        ResourceOperation::Add,
                        i64::MAX,
                    ),
                ]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        for (id, requested, before, actual) in [(1, 3, 3, 2), (2, i64::MAX, 5, 0)] {
            assert_eq!(
                result.receipts[&ReceiptId(id)],
                InstructionOutcome::Resource(ResourceOutcome {
                    player: PlayerId { seat },
                    resource: Resource::PpCurrent,
                    requested: Int(requested),
                    before: Int(before),
                    after: Int(5),
                    actual: Int(actual),
                })
            );
        }
        assert_eq!(result.facts, Vec::<Fact>::new());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[actor]["pp"], json!({"current":5,"max":5}));
        assert_eq!(state[opponent]["pp"], json!({"current":2,"max":2}));
    }
}

#[test]
fn body_resource_admission_rejects_payment_other_resources_and_other_players_atomically() {
    assert!(
        WellFormedProgram::check(body(
            1,
            vec![recovery(
                1,
                PlayerRef::Controller {},
                Resource::PpCurrent,
                ResourceOperation::Pay,
                1,
            )]
        ))
        .is_err(),
        "payment requires a cost context"
    );
    for step in [
        recovery(
            1,
            PlayerRef::Controller {},
            Resource::PpMaximum,
            ResourceOperation::Set,
            1,
        ),
        recovery(
            1,
            PlayerRef::Controller {},
            Resource::Sep,
            ResourceOperation::Add,
            1,
        ),
        recovery(
            1,
            PlayerRef::Active {},
            Resource::PpCurrent,
            ResourceOperation::Add,
            1,
        ),
        recovery(
            1,
            PlayerRef::Controller {},
            Resource::PpCurrent,
            ResourceOperation::Set,
            1,
        ),
        recovery(
            1,
            PlayerRef::Controller {},
            Resource::PpCurrent,
            ResourceOperation::Add,
            -1,
        ),
    ] {
        let mut game = game(1, 0);
        let before = format!("{game:?}");
        game.execute_internal(&checked(vec![step]), &source("synthetic"), context(1))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "rejected body resources must not mutate state"
        );
    }
}

#[test]
fn unproved_signed_resource_amount_is_a_capability_gap_without_mutation() {
    let step = instruction(
        1,
        Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::Ep,
            operation: ResourceOperation::Add,
            amount: Expr::Compute(Box::new(IntOperation::Arithmetic {
                operation: Arithmetic::Subtract,
                arguments: vec![Expr::Literal(Int(0)), Expr::Literal(Int(1))],
            })),
        },
    );
    let mut game = game(1, 0);
    let before = exact_authority(&game);
    let error = game
        .execute_internal(&checked(vec![step]), &source("synthetic"), context(1))
        .unwrap_err();
    assert!(matches!(error, sve_engine::EngineFailure::Unsupported(_)));
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn pp_author_lowering_rejects_opponent_and_to_max() {
    for pp in [
        json!({"who":"opp","delta":3}),
        json!({"who":"self","to_max":true}),
    ] {
        for kind in ["pp", "pp_max", "ep"] {
            let mut tree = fanfare::expanded::tree().clone();
            tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] = json!([{kind:pp}]);
            lowerer().lower(&tree).unwrap_err();
        }
    }
}

#[test]
fn maximum_pp_increases_only_the_cap_and_records_actual_zero_at_ten() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        for maximum in [3_i64, 9, 10] {
            for current in [0, maximum] {
                for requested in [2_i64, i64::MAX] {
                    let mut initial = setup(seat, json!([]));
                    initial["players"][actor]["pp"] = json!({"current":current,"max":maximum});
                    let mut game = Game::new(
                        catalog(),
                        &initial,
                        &Value::Null,
                        &Value::Null,
                        "maximum-pp",
                    )
                    .unwrap();
                    let result = game
                        .execute_internal(
                            &checked(vec![recovery(
                                1,
                                PlayerRef::Controller {},
                                Resource::PpMaximum,
                                ResourceOperation::Add,
                                requested,
                            )]),
                            &source("synthetic"),
                            context(seat),
                        )
                        .unwrap();
                    let actual = requested.min(10_i64.saturating_sub(maximum));
                    let after = maximum.saturating_add(actual);
                    assert_eq!(result.receipts.len(), 1);
                    assert_eq!(
                        result.receipts[&ReceiptId(1)],
                        InstructionOutcome::Resource(ResourceOutcome {
                            player: PlayerId { seat },
                            resource: Resource::PpMaximum,
                            requested: Int(requested),
                            before: Int(maximum),
                            after: Int(after),
                            actual: Int(actual),
                        })
                    );
                    assert!(
                        result.facts.is_empty(),
                        "resource increases do not create payment facts"
                    );
                    let state = game.projection(View::Referee).unwrap();
                    assert_eq!(state[actor]["pp"], json!({"current":current,"max":after}));
                    assert_eq!(state[opponent]["pp"], json!({"current":2,"max":2}));
                }
            }
        }
    }
}

#[test]
fn ep_body_increases_are_uncapped_and_do_not_change_pp_sep_or_the_other_player() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = setup(seat, json!([]));
        initial["players"][actor]["ep"] = json!(2_i64);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "ep-increase",
        )
        .unwrap();
        let result = game
            .execute_internal(
                &checked(vec![
                    recovery(
                        1,
                        PlayerRef::Controller {},
                        Resource::Ep,
                        ResourceOperation::Add,
                        11,
                    ),
                    recovery(
                        2,
                        PlayerRef::Controller {},
                        Resource::Ep,
                        ResourceOperation::Add,
                        1,
                    ),
                ]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        for (id, requested, before, after) in [(1, 11, 2, 13), (2, 1, 13, 14)] {
            assert_eq!(
                result.receipts[&ReceiptId(id)],
                InstructionOutcome::Resource(ResourceOutcome {
                    player: PlayerId { seat },
                    resource: Resource::Ep,
                    requested: Int(requested),
                    before: Int(before),
                    after: Int(after),
                    actual: Int(requested),
                })
            );
        }
        assert_eq!(result.facts, Vec::<Fact>::new());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[actor]["ep"], 14_i64);
        assert_eq!(state[opponent]["ep"], 0_i64);
        assert_eq!(state[actor]["sep"], 0_i64);
        assert_eq!(state[actor]["pp"], json!({"current":2,"max":2}));
    }
}

#[test]
fn ep_body_overflow_and_non_controller_requests_preserve_authority() {
    for seat in [1, 2] {
        for (player, amount, expected) in [
            (PlayerRef::Controller {}, i64::MAX, "integer representation"),
            (PlayerRef::Active {}, 1, "nonnegative own resource increase"),
        ] {
            let actor = if seat == 1 { "P1" } else { "P2" };
            let mut initial = setup(seat, json!([]));
            initial["players"][actor]["ep"] = json!(2_i64);
            let mut game =
                Game::new(catalog(), &initial, &Value::Null, &Value::Null, "ep-reject").unwrap();
            let before = format!("{game:?}");
            let failure = game
                .execute_internal(
                    &checked(vec![recovery(
                        1,
                        player,
                        Resource::Ep,
                        ResourceOperation::Add,
                        amount,
                    )]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap_err();
            if amount == i64::MAX {
                assert!(matches!(failure, sve_engine::EngineFailure::Invariant(_)));
            } else {
                assert!(matches!(failure, sve_engine::EngineFailure::Unsupported(_)));
            }
            assert!(
                failure.to_string().contains(expected),
                "the intended EP boundary rejects"
            );
            assert!(
                format!("{game:?}") == before,
                "rejected EP changes preserve authority"
            );
        }
    }
}

fn amount(id: u64, resource: Resource, amount: Expr<Int>) -> Located<Node> {
    instruction(
        id,
        Action::Resource {
            player: PlayerRef::Controller {},
            resource,
            operation: ResourceOperation::Add,
            amount,
        },
    )
}

#[test]
fn resource_zero_records_completion_without_an_increase_event() {
    for resource in [Resource::PpCurrent, Resource::PpMaximum, Resource::Ep] {
        let mut board = game(1, 0);
        let before = board.projection(View::Referee).unwrap();
        let result = board
            .execute_internal(
                &checked(vec![amount(
                    1,
                    resource,
                    Expr::Compute(Box::new(IntOperation::FromNat {
                        value: Expr::Literal(Nat(0)),
                    })),
                )]),
                &source("synthetic"),
                context(1),
            )
            .unwrap();
        let InstructionOutcome::Resource(outcome) = &result.receipts[&ReceiptId(1)] else {
            panic!("resource receipt")
        };
        assert_eq!(outcome.requested, Int(0));
        assert_eq!(outcome.actual, Int(0));
        assert_eq!(outcome.before, outcome.after);
        assert_eq!(board.projection(View::Referee).unwrap(), before);
        assert_eq!(result.facts, Vec::<Fact>::new());
    }
}

#[test]
fn resource_reads_each_instruction_after_earlier_resource_changes() {
    let count = Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Literal(Nat(2)),
    }));
    let next = Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation: Arithmetic::Maximum,
        arguments: vec![
            Expr::Literal(Int(0)),
            Expr::Live(Box::new(IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::Ep,
            })),
        ],
    }));
    let mut board = game(1, 0);
    let result = board
        .execute_internal(
            &checked(vec![
                amount(1, Resource::Ep, count),
                amount(2, Resource::Ep, next),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    let InstructionOutcome::Resource(first) = result.receipts[&ReceiptId(1)] else {
        panic!("first")
    };
    let InstructionOutcome::Resource(second) = result.receipts[&ReceiptId(2)] else {
        panic!("second")
    };
    assert_eq!(first.actual, Int(2));
    assert_eq!(second.requested, first.after);
    assert_eq!(second.after, Int(second.before.0.checked_mul(2).unwrap()));
}

#[test]
fn resource_formula_overflow_after_a_question_rolls_back_the_whole_execution() {
    let mut board = game(1, 3);
    let original = board.clone();
    let overflow = Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation: Arithmetic::Add,
        arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
    }));
    let progress = board
        .execute_internal(
            &checked(vec![
                draw(1, 1),
                integer_choice(2, 1),
                amount(3, Resource::PpCurrent, overflow),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert!(!progress.complete);
    let question = board.internal_decision(View::P1).unwrap();
    let published = board.clone();
    let result = board.answer_internal(
        PlayerId { seat: 1 },
        question.token,
        Answer::Integer(Nat(0)),
    );
    assert!(matches!(result, Err(sve_engine::EngineFailure::Invalid(_))));
    transactions::assert_reverted_from(&board, &original, &published);
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture shapes cannot define a resource IR scenario."
)]
fn filtered_mill_resource() -> CardProgram {
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[
            {"move":{"top":{"who":"self","n":3},"to":"cemetery","result":"milled"}},
            {"damage":{"to":{"leaders":"opponents"},"n":{"count_of":{"receipt":"milled","where":{"trait":"trait-a"},"view":"before"}}}}
        ]}}),
        "unit-follower",
    );
    let mut card = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    let Node::Instruction { instruction } =
        &mut card.abilities[0].node.body.as_mut().unwrap().steps[1].node
    else {
        panic!("consumer")
    };
    let Action::Damage {
        amount: DamageAmount::Uniform { amount },
        ..
    } = &instruction.action
    else {
        panic!("uniform amount")
    };
    instruction.action = Action::Resource {
        player: PlayerRef::Controller {},
        resource: Resource::PpCurrent,
        operation: ResourceOperation::Add,
        amount: amount.clone(),
    };
    card
}

#[test]
fn resource_ir_reads_only_matching_successful_mill_members() {
    let program = WellFormedCardProgram::check(filtered_mill_resource()).unwrap();
    let (snapshot, registry, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-spell" {
                row["faces"][0]["traits"] = json!(["trait-a"]);
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalogue = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    for (deck, requested) in [
        (json!([]), 0),
        (json!([{"id":"d0","card":"unit-spell"}]), 1),
        (
            json!([{"id":"d0","card":"unit-spell"},{"id":"d1","card":"unit-follower"},{"id":"d2","card":"unit-spell"},{"id":"tail","card":"unit-spell"}]),
            2,
        ),
    ] {
        let mut initial = setup(1, deck.clone());
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["pp"] = json!({"current":0,"max":5});
        let mut board = Game::new(
            Arc::clone(&catalogue),
            &initial,
            &Value::Null,
            &Value::Null,
            "resource-IR-mill",
        )
        .unwrap();
        board.load_internal_card("unit-follower", &program).unwrap();
        for number in ["unit-spell", "unit-amulet"] {
            board.load_internal_card(number, &card(vec![])).unwrap();
        }
        let result = resolved(
            board
                .activate_internal_card(
                    "s",
                    program.card().abilities[0].node.id,
                    PlayerId { seat: 1 },
                )
                .unwrap(),
        );
        let outcome = result
            .progress
            .receipts
            .values()
            .find_map(|receipt| {
                if let InstructionOutcome::Resource(value) = receipt {
                    Some(value)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(outcome.requested, Int(requested));
        assert_eq!(outcome.actual, Int(requested));
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            requested
        );
        if deck.as_array().unwrap().len() > 3 {
            let state = board.projection(View::P2).unwrap();
            assert!(
                !state["known_cards"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|card| card["id"] == "tail")
            );
        }
    }
}

#[test]
fn resource_ir_does_not_accept_a_forged_mill_producer() {
    let original = filtered_mill_resource();
    for (case, nesting) in ["field", "opponent", "ex"].into_iter().flat_map(|case| {
        ["direct", "conditional", "natural"]
            .into_iter()
            .map(move |nesting| (case, nesting))
    }) {
        let mut changed = original.clone();
        let Node::Instruction { instruction } =
            &mut changed.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("producer")
        };
        let Action::Move {
            objects, movement, ..
        } = &mut instruction.action
        else {
            panic!("move")
        };
        match case {
            "field" => {
                *objects = Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                }
            }
            "opponent" => {
                let Selector::Ordered { zone, .. } = objects else {
                    panic!("deck")
                };
                zone.player = PlayerRef::RequireOne {
                    players: Box::new(PlayerSet {
                        relation: PlayerRelation::Opponents,
                        relative_to: PlayerRef::Controller {},
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        text_quantifier: TextQuantifier::One,
                    }),
                };
            }
            _ => {
                movement.to = MoveDestination::Owners {
                    zone: OwnerZone::Ex,
                }
            }
        }
        if nesting != "direct" {
            let Node::Instruction { instruction } =
                &mut changed.abilities[0].node.body.as_mut().unwrap().steps[1].node
            else {
                panic!("consumer")
            };
            let Action::Resource { amount, .. } = &mut instruction.action else {
                panic!("resource")
            };
            *amount = if nesting == "conditional" {
                Expr::Compute(Box::new(IntOperation::Conditional {
                    condition: Expr::Literal(Bool(true)),
                    then_value: amount.clone(),
                    else_value: Expr::Literal(Int(0)),
                }))
            } else {
                Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Compute(Box::new(NatOperation::CheckedInt {
                        value: amount.clone(),
                    })),
                }))
            };
        }
        let mut control = changed.clone();
        let Node::Instruction { instruction } =
            &mut control.abilities[0].node.body.as_mut().unwrap().steps[1].node
        else {
            panic!("consumer")
        };
        let Action::Resource { amount, .. } = &mut instruction.action else {
            panic!("resource")
        };
        *amount = Expr::Literal(Int(0));
        let mut baseline = game(1, 0);
        baseline
            .load_internal_card(
                "unit-follower",
                &WellFormedCardProgram::check(control).unwrap(),
            )
            .unwrap();
        let mut board = game(1, 0);
        let before = exact_authority(&board);
        let failure = board
            .load_internal_card(
                "unit-follower",
                &WellFormedCardProgram::check(changed).unwrap(),
            )
            .unwrap_err();
        assert!(matches!(failure, sve_engine::EngineFailure::Unsupported(_)));
        assert_eq!(exact_authority(&board), before);
    }
}
