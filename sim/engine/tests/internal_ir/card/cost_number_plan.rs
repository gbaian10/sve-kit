use super::*;
use sve_engine::EngineFailure;
use sve_engine::game::player::Viewer;

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
