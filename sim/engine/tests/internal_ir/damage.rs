use super::*;

fn requested(id: u64) -> Expr<Int> {
    Expr::Frozen(Box::new(IntFrozen::ReceiptRequested {
        receipt: MeasuredReceipt::Damage {
            receipt: ReceiptReference::new(ReceiptId(id)),
        },
    }))
}

fn actual(id: u64) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Frozen(Box::new(NatFrozen::ReceiptActual {
            receipt: MeasuredReceipt::Damage {
                receipt: ReceiptReference::new(ReceiptId(id)),
            },
        })),
    }))
}

fn outcome(progress: &Progress, id: u64, requested: i64, actual: u64) {
    assert_eq!(
        progress.receipts.get(&ReceiptId(id)),
        Some(&InstructionOutcome::Damage(DamageQuantityOutcome {
            requested: Int(requested),
            actual: Nat(actual),
        })),
        "damage receipt {id}"
    );
}

#[test]
fn aggregate_projections_deduplicate_and_do_not_cap_damage_at_remaining_life() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let deck = (0..20)
            .map(|index| json!({"id":format!("d{index}"),"card":"unit-follower"}))
            .collect::<Vec<_>>();
        let mut position = setup(seat, json!(deck));
        position["players"][opponent]["leader"]["life"] = json!(1);
        position["players"][opponent]["zones"]["field"] = json!([
            {"id":"f","card":"unit-follower"},
            {"id":"g","card":"unit-follower"}
        ]);
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "aggregate",
        )
        .unwrap();
        let result = game
            .execute_internal(
                &checked(vec![
                    damage(
                        1,
                        // The second set repeats the same members under its own selector scope.
                        RecipientSet::Union {
                            sets: vec![
                                recipients(),
                                RecipientSet::Union {
                                    sets: vec![
                                        RecipientSet::Leaders {
                                            players: players(PlayerRelation::Opponents),
                                        },
                                        RecipientSet::Objects {
                                            objects: zone(ZoneKind::Field, 98),
                                        },
                                    ],
                                },
                            ],
                        },
                        3,
                    ),
                    draw_value(2, requested(1)),
                    draw_value(3, actual(1)),
                ]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        outcome(&result, 1, 9, 9);
        cards(&result, 2, 9, 9);
        cards(&result, 3, 9, 9);
        let hits = result
            .facts
            .iter()
            .filter_map(|fact| {
                if let Fact::Damage(hit) = fact {
                    Some(hit)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 3);
        assert!(hits.iter().all(|hit| hit.amount.0.get() == 3));
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection[opponent]["leader"]["life"], -2);
        // Leader defeat ends the game before the existing field cleanup can run.
        assert_eq!(
            projection["game"],
            json!({"ended":true,"winner":actor,"by":"11.2.1"})
        );
    }
}

#[test]
fn stale_captured_members_keep_requested_but_not_actual_damage() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let mut position = setup(
            seat,
            json!(
                (0..10)
                    .map(|index| json!({"id":format!("d{index}"),"card":"unit-follower"}))
                    .collect::<Vec<_>>()
            ),
        );
        position["players"][actor]["zones"]["field"] = json!([{"id":"a","card":"unit-follower"}]);
        let mut game =
            Game::new(catalog(), &position, &Value::Null, &Value::Null, "stale").unwrap();
        let binding = Binding::new(1, ScopeId(1));
        let own_field = Selector::Zone {
            players: players(PlayerRelation::SelfPlayer),
            zone: ZoneKind::Field,
            view: SelectionView::Current {},
            binding: Binding::new(9, ScopeId(98)),
            predicate: Box::new(Expr::Literal(Bool(true))),
        };
        let result = game
            .execute_internal(
                &checked(vec![
                    node(Node::Capture {
                        capture: Capture::Objects {
                            at: CaptureTime::AtResolution { scope: ScopeId(1) },
                            value: zone(ZoneKind::Field, 99),
                            into: CaptureRef { binding },
                        },
                    }),
                    movement::put(1, own_field, OwnerZone::Banish),
                    damage(
                        2,
                        RecipientSet::Objects {
                            objects: Selector::Captured {
                                reference: CaptureRef { binding },
                            },
                        },
                        2,
                    ),
                    draw_value(3, requested(2)),
                    draw_value(4, actual(2)),
                    damage(
                        5,
                        RecipientSet::Objects {
                            objects: zone(ZoneKind::Field, 97),
                        },
                        1,
                    ),
                ]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        outcome(&result, 2, 4, 2);
        outcome(&result, 5, 1, 1);
        cards(&result, 3, 4, 4);
        cards(&result, 4, 2, 2);
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection[actor]["banish_count"], 1);
    }
}

#[test]
fn negative_zero_and_empty_damage_project_zero_and_continue() {
    for seat in [1, 2] {
        for (targets, amount) in [
            (recipients(), -2),
            (recipients(), 0),
            (RecipientSet::Explicit { recipients: vec![] }, 3),
        ] {
            let mut game = game(seat, 1);
            let result = game
                .execute_internal(
                    &checked(vec![
                        damage(1, targets, amount),
                        draw_value(2, requested(1)),
                        draw_value(3, actual(1)),
                        draw(4, 1),
                    ]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap();
            outcome(&result, 1, 0, 0);
            cards(&result, 2, 0, 0);
            cards(&result, 3, 0, 0);
            cards(&result, 4, 1, 1);
            assert!(matches!(result.facts.as_slice(), [Fact::Draw(_)]));
        }
    }
}

#[test]
fn requested_overflow_rolls_back_prior_draw_and_all_observations() {
    for seat in [1, 2] {
        let mut game = game(seat, 2);
        let before = exact_authority(&game);
        let views = [View::P1, View::P2, View::Referee];
        let observations = views.map(|view| game.projection(view).unwrap());
        let error = game
            .execute_internal(
                &checked(vec![draw(1, 1), damage(2, recipients(), i64::MAX)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap_err();
        assert!(error.to_string().contains("damage requested overflow"));
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            views.map(|view| game.projection(view).unwrap()),
            observations
        );
    }
}

#[test]
fn damage_state_overflow_rejects_instead_of_saturating() {
    for seat in [1, 2] {
        let opponent = if seat == 1 { "P2" } else { "P1" };
        for (leader, hp, prior_damage) in [
            (true, i64::MIN, 0),
            (false, i64::MIN, 0),
            (false, 3, i64::MAX),
        ] {
            let game = game(seat, 1);
            let mut saved = serde_json::to_value(&game).unwrap();
            saved["state"]["players"][opponent]["leader"]["life"] = json!(hp);
            saved["state"]["objects"]["f"]["state"]["hp"] = json!(hp);
            saved["state"]["objects"]["f"]["state"]["damage"] = json!(prior_damage);
            let mut game: Game = serde_json::from_value(saved).unwrap();
            let before = exact_authority(&game);
            let targets = if leader {
                RecipientSet::Leaders {
                    players: players(PlayerRelation::Opponents),
                }
            } else {
                RecipientSet::Objects {
                    objects: zone(ZoneKind::Field, 99),
                }
            };
            let error = game
                .execute_internal(
                    &checked(vec![draw(1, 1), damage(2, targets, 1)]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("damage state arithmetic overflow")
            );
            assert_eq!(exact_authority(&game), before);
        }
    }
}

#[test]
fn damage_and_card_receipt_types_cannot_be_substituted() {
    let card_as_damage =
        WellFormedProgram::check(body(1, vec![draw(1, 1), draw_value(2, requested(1))]))
            .unwrap_err();
    assert_eq!(card_as_damage.reason, WellFormedReason::ReceiptType);
    let damage_as_card = WellFormedProgram::check(body(
        1,
        vec![
            damage(1, recipients(), 1),
            draw_value(
                2,
                Expr::Frozen(Box::new(IntFrozen::ReceiptRequested {
                    receipt: MeasuredReceipt::Cards {
                        receipt: ReceiptReference::new(ReceiptId(1)),
                    },
                })),
            ),
        ],
    ))
    .unwrap_err();
    assert_eq!(damage_as_card.reason, WellFormedReason::ReceiptType);
}

fn reject_damage_domain(
    seat: u32,
    card: &str,
    domain: (ZoneKind, &str),
    captured: bool,
    amount: i64,
    expected: &str,
) {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let (kind, zone_name) = domain;
    let mut position = setup(seat, json!([{"id":"d","card":"unit-follower"}]));
    position["players"][actor]["zones"][zone_name] = json!([{"id":"a","card":card}]);
    let mut game = Game::new(
        catalog(),
        &position,
        &Value::Null,
        &Value::Null,
        "unsupported-damage-domain",
    )
    .unwrap();
    let before = exact_authority(&game);
    let views = [View::P1, View::P2, View::Referee];
    let observations = views.map(|view| game.projection(view).unwrap());
    let selected = Selector::Zone {
        players: players(PlayerRelation::SelfPlayer),
        zone: kind,
        view: SelectionView::Current {},
        binding: Binding::new(9, ScopeId(99)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    };
    let mut steps = Vec::new();
    let objects = if captured {
        let binding = Binding::new(1, ScopeId(1));
        steps.push(node(Node::Capture {
            capture: Capture::Objects {
                at: CaptureTime::AtResolution { scope: ScopeId(1) },
                value: selected,
                into: CaptureRef { binding },
            },
        }));
        steps.push(draw(1, 1));
        let captured = Selector::Captured {
            reference: CaptureRef { binding },
        };
        steps.push(movement::put(
            2,
            captured.clone(),
            if kind == ZoneKind::Cemetery {
                OwnerZone::Banish
            } else {
                OwnerZone::Cemetery
            },
        ));
        captured
    } else {
        steps.push(draw(1, 1));
        selected
    };
    steps.push(damage(3, RecipientSet::Objects { objects }, amount));
    steps.push(draw_value(4, requested(3)));
    let error = game
        .execute_internal(&checked(steps), &source("synthetic"), context(seat))
        .unwrap_err();
    assert!(
        matches!(&error, sve_engine::EngineFailure::Unsupported(_)),
        "{expected}: expected Unsupported"
    );
    assert!(
        error.to_string().contains(expected),
        "expected {expected}, got {error}"
    );
    assert_eq!(
        exact_authority(&game),
        before,
        "{expected}: authority rollback"
    );
    assert_eq!(
        views.map(|view| game.projection(view).unwrap()),
        observations,
        "{expected}: all-view rollback"
    );
}

#[test]
fn non_follower_members_reject_the_whole_attempt_even_after_capture_and_movement() {
    for seat in [1, 2] {
        for captured in [false, true] {
            for amount in [0, 2] {
                reject_damage_domain(
                    seat,
                    "unit-amulet",
                    (ZoneKind::Field, "field"),
                    captured,
                    amount,
                    "non-follower damage recipient",
                );
            }
        }
    }
}

#[test]
fn non_field_domains_reject_without_confusing_stale_field_captures() {
    for seat in [1, 2] {
        for captured in [false, true] {
            for domain in [
                (ZoneKind::Ex, "ex"),
                (ZoneKind::Cemetery, "cemetery"),
                (ZoneKind::Banish, "banish"),
            ] {
                for amount in [0, 2] {
                    reject_damage_domain(
                        seat,
                        "unit-follower",
                        domain,
                        captured,
                        amount,
                        if captured {
                            "non-field damage capture"
                        } else {
                            "non-field damage recipient"
                        },
                    );
                }
            }
        }
    }
}

#[test]
fn overflowing_answer_restores_execution_and_accepts_a_fresh_attempt() {
    for seat in [1, 2] {
        let mut game = game(seat, 6);
        let program = checked(vec![
            draw(1, 1),
            integer_choice_within(
                1,
                1,
                Cardinality {
                    minimum: Nat(0),
                    maximum: Some(Nat(u64::try_from(i64::MAX).unwrap())),
                },
            ),
            instruction(
                2,
                Action::Damage {
                    recipients: recipients(),
                    amount: DamageAmount::Uniform {
                        amount: signed_binding(1),
                    },
                    flags: DamageFlags::Ability,
                    source: ObjectView::Current {
                        object: Box::new(ObjectOperand::SourceHolder {}),
                    },
                    batch: BatchPolicy::Instruction {},
                },
            ),
            draw_value(3, actual(2)),
        ]);
        let origin = game.clone();
        let initial = game
            .execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        assert!(!initial.complete);
        let question = game.internal_decision(View::Referee).unwrap();
        game.answer_internal(
            PlayerId { seat },
            question.token,
            Answer::Integer(Nat(u64::try_from(i64::MAX).unwrap())),
        )
        .unwrap_err();
        transactions::assert_restored(&game, &origin);
        game.execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        let question = game.internal_decision(View::Referee).unwrap();
        let result = game
            .answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(2)))
            .unwrap();
        outcome(&result, 2, 4, 4);
        cards(&result, 1, 1, 1);
        cards(&result, 3, 4, 4);
        let before_replay = exact_authority(&game);
        game.answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(2)))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before_replay);
    }
}

#[cfg(feature = "runner")]
#[test]
fn printed_bp02_052_uniform_fragment_aggregates_three_frozen_hand_cards() {
    use std::env::var_os;
    use std::fs::read_to_string;
    use std::path::PathBuf;

    let snapshot = read_to_string(PathBuf::from(
        var_os("SVE_TEST_SNAPSHOT").expect("generate public card input first"),
    ))
    .unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    // Only the fixed-count damage fragment is exercised; evolution and discarding remain outside it.
    let fragments = json!({"format":1,"kind":"effect_set","cards":{
        "BP02-052":{"status":"complete","review":"synthetic fragment harness","abilities":[]},
        "BP01-006":{"status":"complete","review":"synthetic fragment harness","abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("fragments.yaml".into(), fragments)],
        )
        .unwrap(),
    );
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut position = setup(seat, json!([]));
        position["players"][actor]["zones"]["hand"] = json!([
            {"id":"h1","card":"BP01-006"},
            {"id":"h2","card":"BP01-006"},
            {"id":"h3","card":"BP01-006"}
        ]);
        position["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"BP02-052"}]);
        position["players"][opponent]["zones"]["field"] = json!([
            {"id":"f","card":"BP01-006"},
            {"id":"g","card":"BP01-006"}
        ]);
        let objects = Selector::Zone {
            players: players(PlayerRelation::Opponents),
            zone: ZoneKind::Field,
            view: SelectionView::Current {},
            binding: Binding::new(9, ScopeId(99)),
            predicate: Box::new(Expr::Literal(Bool(true))),
        };
        let targets = RecipientSet::Union {
            sets: vec![
                RecipientSet::Objects { objects },
                RecipientSet::Leaders {
                    players: players(PlayerRelation::Opponents),
                },
            ],
        };
        let mut game = Game::new(
            Arc::clone(&catalog),
            &position,
            &Value::Null,
            &Value::Null,
            "BP02-052-fragment",
        )
        .unwrap();
        let result = game
            .execute_internal(
                &checked(vec![damage(1, targets, 3)]),
                &source("BP02-052"),
                context(seat),
            )
            .unwrap();
        outcome(&result, 1, 9, 9);
        assert_eq!(result.facts.len(), 3);
        assert!(
            result
                .facts
                .iter()
                .all(|fact| matches!(fact, Fact::Damage(hit) if hit.amount.0.get() == 3))
        );
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection[opponent]["leader"]["life"], 17);
        assert_eq!(projection[actor]["hand_count"], 3);
    }
}

#[test]
fn union_cannot_hide_an_off_field_capture_and_rolls_back_every_prior_effect() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let mut position = setup(seat, json!([{ "id":"d","card":"unit-follower" }]));
        position["players"][actor]["zones"]["cemetery"] =
            json!([{ "id":"a","card":"unit-follower" }]);
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "union-capture",
        )
        .unwrap();
        let before = exact_authority(&game);
        let views = [View::P1, View::P2, View::Referee];
        let observations = views.map(|view| game.projection(view).unwrap());
        let binding = Binding::new(1, ScopeId(1));
        let captured = Selector::Captured {
            reference: CaptureRef { binding },
        };
        let program = checked(vec![
            node(Node::Capture {
                capture: Capture::Objects {
                    at: CaptureTime::AtResolution { scope: ScopeId(1) },
                    value: zone(ZoneKind::Cemetery, 90),
                    into: CaptureRef { binding },
                },
            }),
            draw(1, 1),
            movement::put(2, captured.clone(), OwnerZone::Banish),
            damage(
                3,
                RecipientSet::Objects {
                    objects: Selector::Union {
                        sets: vec![captured, zone(ZoneKind::Field, 91)],
                    },
                },
                2,
            ),
        ]);
        let error = game
            .execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap_err();
        assert!(matches!(&error, sve_engine::EngineFailure::Unsupported(_)));
        assert!(error.to_string().contains("non-field damage capture"));
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            views.map(|view| game.projection(view).unwrap()),
            observations
        );
    }
}
