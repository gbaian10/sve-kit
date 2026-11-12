use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::player::Viewer;
use sve_engine::ir::lowering::LoweringError;

fn raw(amount: Expr<Int>, condition: Option<Expr<Bool>>) -> CardProgram {
    let source = source("unit-follower");
    CardProgram {
        abilities: vec![],
        card_play_costs: vec![Located {
            source: source.clone(),
            node: FacePlayCost {
                face: FaceId("0".into()),
                additional: None,
                exchange: None,
                modifiers: vec![Located {
                    source,
                    node: PlayCostModifier {
                        condition,
                        change: CostChange::Delta { amount },
                    },
                }],
            },
        }],
    }
}

fn initial() -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial
}

fn board(initial: &Value) -> Game {
    Game::new(
        catalog_with_cost(&json!("3")),
        initial,
        &Value::Null,
        &Value::Null,
        "cost-number-plan",
    )
    .unwrap()
}

fn overflow() -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation: Arithmetic::Multiply,
        arguments: vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(2))],
    }))
}

#[test]
fn inactive_cost_does_not_evaluate_an_overflowing_delta_in_quote_or_payment() {
    let program =
        WellFormedCardProgram::check(raw(overflow(), Some(Expr::Literal(Bool(false))))).unwrap();
    let mut game = board(&initial());
    game.load_internal_card("unit-follower", &program).unwrap();
    assert_eq!(
        cost_observation::preview(&game, Viewer::P1).cost.total_pp,
        Some(3)
    );
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        7
    );
}

#[test]
fn an_overflowing_delta_is_admitted_and_fails_only_when_quoted_for_payment() {
    let program = WellFormedCardProgram::check(raw(overflow(), None)).unwrap();
    let mut game = board(&initial());
    game.load_internal_card("unit-follower", &program).unwrap();
    let before = exact_authority(&game);
    assert_eq!(
        cost_observation::preview(&game, Viewer::P1).cost.total_pp,
        None
    );
    let error = game
        .play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::Invalid(message) if message == "signed arithmetic overflow")
    );
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn cost_counts_keep_opponent_and_both_banish_populations_for_quote_and_payment() {
    for (relation, expected) in [(PlayerRelation::Opponents, 2), (PlayerRelation::All, 1)] {
        let count = Expr::Live(Box::new(NatRead::ZoneCount {
            selector: Selector::Zone {
                players: players(relation),
                zone: ZoneKind::Banish,
                view: SelectionView::Current {},
                binding: Binding::new(90, ScopeId(0)),
                predicate: Box::new(Expr::Literal(Bool(true))),
            },
        }));
        let program = WellFormedCardProgram::check(raw(
            Expr::Compute(Box::new(IntOperation::Arithmetic {
                operation: Arithmetic::Subtract,
                arguments: vec![
                    Expr::Literal(Int(0)),
                    Expr::Compute(Box::new(IntOperation::FromNat { value: count })),
                ],
            })),
            None,
        ))
        .unwrap();
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["banish"] = json!([{"id":"own","card":"unit-spell"}]);
        initial["players"]["P2"]["zones"]["banish"] =
            json!([{"id":"opponent","card":"unit-spell"}]);
        let mut game = board(&initial);
        game.load_internal_card("unit-follower", &program).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        assert_eq!(
            cost_observation::preview(&game, Viewer::P1).cost.total_pp,
            Some(expected)
        );
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            10 - expected
        );
    }
}

#[test]
fn cost_counts_do_not_inherit_field_selector_object_or_count_permissions() {
    let objects = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    for value in [
        Expr::Live(Box::new(NatRead::ZoneCount {
            selector: objects.clone(),
        })),
        Expr::Compute(Box::new(NatOperation::Count { objects })),
    ] {
        let program = WellFormedCardProgram::check(raw(
            Expr::Compute(Box::new(IntOperation::FromNat { value })),
            None,
        ))
        .unwrap();
        let mut game = board(&initial());
        assert!(matches!(
            game.load_internal_card("unit-follower", &program),
            Err(EngineFailure::Unsupported(_))
        ));
    }
    for selector in [
        Selector::Bound {
            reference: Binding::new(91, ScopeId(0)),
        },
        Selector::Captured {
            reference: CaptureRef {
                binding: Binding::new(91, ScopeId(0)),
            },
        },
    ] {
        WellFormedCardProgram::check(raw(
            Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Live(Box::new(NatRead::ZoneCount { selector })),
            })),
            None,
        ))
        .unwrap_err();
    }
}

#[test]
fn a_static_cost_condition_uses_its_providers_controller_during_quote_and_payment() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["static"] = json!({"while":{"leader_life":{"who":"self","le":10}},"effects":[{"cost_mod":{"when_playing":{"who":"opp","kind":"follower"},"delta":-2}}]});
    tree["cards"] =
        json!({"unit-follower":{"meta":tree["cards"]["BP08-038"]["meta"],"abilities":[entry]}});
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for (life, expected) in [(9, 1), (11, 3)] {
        let mut initial = initial();
        initial["players"]["P2"]["leader"]["life"] = json!(life);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"provider","card":"unit-follower"}]);
        let mut game = board(&initial);
        game.load_internal_card("unit-follower", &program).unwrap();
        assert_eq!(
            cost_observation::preview(&game, Viewer::P1).cost.total_pp,
            Some(expected)
        );
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            10 - expected
        );
    }
}

fn lower_head(head: &str, body: Value) -> Result<WellFormedCardProgram, LoweringError> {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry[head] = body;
    tree["cards"] = json!({"unit-follower":{
        "meta":tree["cards"]["BP08-038"]["meta"],"abilities":[entry]
    }});
    Ok(lowerer().lower(&tree)?.remove("unit-follower").unwrap())
}

#[test]
fn cost_history_permissions_admit_only_discarded_and_declared_subject_facts() {
    for (event, admitted) in [
        ("discarded", true),
        ("attacked", true),
        ("evolved", true),
        ("left_field", true),
        ("ub_activated", false),
        ("destroyed", false),
    ] {
        let mut query = json!({"event":event,"window":"this_turn","who":"self"});
        match event {
            "discarded" => query["from"] = json!("hand"),
            "ub_activated" => {}
            _ => query["sel"] = json!({"kind":"follower"}),
        }
        let body = lower_head(
            "activate",
            json!({"cost":{"pp":0},
            "do":[{"draw":{"n":{"history_count":query}}}]}),
        )
        .unwrap();
        board(&initial())
            .load_internal_card("unit-follower", &body)
            .unwrap();
        let cost = lower_head("play", json!({"cost_delta":{"history_count":query}})).unwrap();
        let mut game = board(&initial());
        if admitted {
            game.load_internal_card("unit-follower", &cost).unwrap();
        } else {
            assert!(
                matches!(
                    game.load_internal_card("unit-follower", &cost),
                    Err(EngineFailure::Unsupported(_))
                ),
                "{event} remains outside Cost"
            );
        }
    }
}

#[test]
fn stat_gain_history_remains_an_evolution_qualification_not_a_cost_or_body_condition() {
    let condition = json!({"history":{"event":"stat_changed","window":"this_turn",
        "where":{"ref":"this","direction":"up","stat":["power","hp"]}}});
    lower_head("evolve", json!({"cost":{"pp":1},"usable_if":condition})).unwrap();
    assert_eq!(
        lower_head("play", json!({"cost_delta":{"if":condition,"delta":-1}}))
            .unwrap_err()
            .code,
        "unsupported_construct"
    );
    assert_eq!(
        lower_head(
            "activate",
            json!({"cost":{"pp":0},
        "do":[{"if":{"cond":condition,"then":[{"draw":{"n":1}}]}}]}),
        )
        .unwrap_err()
        .code,
        "unsupported_construct"
    );
}

#[test]
fn cost_subject_histories_do_not_admit_subject_deduplication_or_other_event_conditions() {
    for event in ["attacked", "evolved", "left_field"] {
        let query = json!({"event":event,"window":"this_turn","who":"self","distinct":"subjects"});
        let program = lower_head("play", json!({"cost_delta":{"history_count":query}})).unwrap();
        assert!(matches!(
            board(&initial()).load_internal_card("unit-follower", &program),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(
            lower_head(
                "play",
                json!({"cost_delta":{"if":{"history":query},"delta":-1}})
            )
            .unwrap_err()
            .code,
            "unsupported_construct"
        );
    }
    for event in ["ub_activated", "played", "destroyed"] {
        assert_eq!(
            lower_head(
                "play",
                json!({"cost_delta":{"if":{"history":{
            "event":event,"window":"this_turn","who":"self"
        }},"delta":-1}})
            )
            .unwrap_err()
            .code,
            "unsupported_construct"
        );
    }
}

#[test]
fn cost_subject_histories_reject_original_cost_and_fragment_names() {
    for selector in [
        json!({"kind":"follower","orig_cost_eq":1}),
        json!({"kind":"follower","name_contains":"unit"}),
    ] {
        let query = json!({"event":"left_field","window":"this_turn","who":"self","sel":selector});
        assert_eq!(
            lower_head(
                "play",
                json!({"cost_delta":{"if":{"history":query},"delta":-1}})
            )
            .unwrap_err()
            .code,
            "unsupported_construct"
        );
        assert_eq!(
            lower_head("play", json!({"cost_delta":{"history_count":query}}))
                .unwrap_err()
                .code,
            "unsupported_construct"
        );
    }
}
