use super::*;

fn own_costs(changes: Vec<CostChange>) -> WellFormedCardProgram {
    let source = source("unit-follower");
    WellFormedCardProgram::check(CardProgram {
        abilities: vec![],
        card_play_costs: vec![Located {
            source: source.clone(),
            node: FacePlayCost {
                exchange: None,
                face: FaceId("0".into()),
                modifiers: changes
                    .into_iter()
                    .map(|change| Located {
                        source: source.clone(),
                        node: PlayCostModifier {
                            condition: None,
                            change,
                        },
                    })
                    .collect(),
            },
        }],
    })
    .unwrap()
}

#[test]
fn set_then_independent_deltas_pay_four_and_negative_raw_never_restores_pp() {
    let program = own_costs(vec![
        CostChange::Delta {
            amount: Expr::Literal(Int(-2)),
        },
        CostChange::Set {
            amount: Expr::Literal(Int(7)),
        },
        CostChange::Delta {
            amount: Expr::Literal(Int(-1)),
        },
    ]);
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "set-cost").unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0
    );
    let mut free = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "free-cost").unwrap();
    free.load_internal_card(
        "unit-follower",
        &own_costs(vec![CostChange::Delta {
            amount: Expr::Literal(Int(-2)),
        }]),
    )
    .unwrap();
    free.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    assert_eq!(
        free.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4
    );
}

#[test]
fn explicit_cemetery_intervals_use_payment_state_and_keep_independent_deltas() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    let count = json!({"count":{"who":"self","zone":"cemetery","kind":"spell"}});
    let threshold = |op, right| json!({"cmp":{"left":count,"op":op,"right":right}});
    entry["play"] = json!({"cost_delta":[
        {"if":threshold("ge",15),"delta":-9},
        {"if":{"all":[threshold("ge",10),threshold("lt",15)]},"delta":-6},
        {"if":{"all":[threshold("ge",5),threshold("lt",10)]},"delta":-3},-1,-1]});
    let mut seed = tree["cards"]["BP08-038"].clone();
    seed["abilities"] = json!([entry]);
    tree["cards"] = json!({"unit-follower":seed});
    let cards = lowerer().lower(&tree).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0..10)
            .map(|i| json!({"id":format!("g{i}"),"card":"unit-spell"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog_with_cost(&json!("10")),
        &initial,
        &Value::Null,
        &Value::Null,
        "interval-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &cards["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        8
    );
}

#[test]
fn ex_count_cost_uses_own_matching_public_cards() {
    for present in [false, true] {
        let mut tree = fanfare::expanded::tree().clone();
        let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
        entry.as_object_mut().unwrap().remove("fanfare");
        entry["play"] = json!({"cost_delta":{"if":{"cmp":{"left":{"count":{"who":"self","zone":"ex","kind":"spell","name_contains":"unit-"}},"op":"ge","right":2_u8}},"delta":-2_i8}});
        let mut seed = tree["cards"]["BP08-038"].clone();
        seed["abilities"] = json!([entry]);
        tree["cards"] = json!({"unit-follower":seed});
        let programs = lowerer().lower(&tree).unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"wrong-kind","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"opponent-one","card":"unit-spell"},{"id":"opponent-two","card":"unit-spell"}]);
        if present {
            for i in 0..2_u8 {
                initial["players"]["P1"]["zones"]["ex"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("matching-{i}"),"card":"unit-spell"}));
            }
        }
        let mut game = Game::new(
            catalog_with_cost(&json!("3")),
            &initial,
            &Value::Null,
            &Value::Null,
            "ex-cost",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if present { 9_u8 } else { 7_u8 }
        );
    }
}

fn conditional_program(condition: &Value, spell: bool) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["play"] = json!({"cost_delta":{"if":condition,"delta":-2_i8}});
    let mut seed = tree["cards"]["BP08-038"].clone();
    seed["abilities"] = json!([entry]);
    let name = if spell { "unit-spell" } else { "unit-follower" };
    tree["cards"] = json!({name:seed});
    lowerer().lower(&tree).unwrap().remove(name).unwrap()
}

fn discounted_game(condition: &Value, grave: Value, maximum: u8, life: u8) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":maximum});
    initial["players"]["P1"]["leader"]["life"] = json!(life);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = grave;
    initial["players"]["P2"]["zones"]["cemetery"] =
        json!([{"id":"opponent-spell","card":"unit-spell"}]);
    let mut game = Game::new(
        catalog_with_cost(&json!("3")),
        &initial,
        &Value::Null,
        &Value::Null,
        "qualified-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &conditional_program(condition, false))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn spellchain_discount_counts_only_own_cemetery_spells_at_payment() {
    for spells in [2_u8, 3] {
        let mut grave = vec![json!({"id":"nonspell","card":"unit-follower"})];
        grave.extend((0..spells).map(|i| json!({"id":format!("spell-{i}"),"card":"unit-spell"})));
        let mut game = discounted_game(&json!({"spellchain":3}), json!(grave), 10, 20);
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if spells == 3 { 9_u8 } else { 7_u8 }
        );
    }
}

#[test]
fn necro_discount_counts_all_cemetery_cards_without_spending_them() {
    for size in [2_u8, 3] {
        let grave = json!(
            (0..size)
                .map(|i| json!({"id":format!("grave-{i}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let mut game = discounted_game(&json!({"necro":3}), grave, 10, 20);
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if size == 3 { 9_u8 } else { 7_u8 }
        );
        assert_eq!(
            state["P1"]["cemetery"].as_array().unwrap().len(),
            usize::from(size)
        );
    }
}

#[test]
fn awakening_discount_reads_maximum_pp_before_payment() {
    for maximum in [6_u8, 7] {
        let mut game = discounted_game(&json!({"awakened":true}), json!([]), maximum, 20);
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if maximum == 7 { 9_u8 } else { 7_u8 }
        );
    }
}

#[test]
fn leader_life_discount_checks_the_printed_threshold() {
    for life in [9_u8, 10, 11] {
        let mut game = discounted_game(
            &json!({"leader_life":{"who":"self","le":10}}),
            json!([]),
            10,
            life,
        );
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if life <= 10 { 9_u8 } else { 7_u8 }
        );
    }
}

#[test]
fn opponent_life_condition_does_not_read_the_controllers_life() {
    let mut game = discounted_game(
        &json!({"leader_life":{"who":"opp","ge":20}}),
        json!([]),
        10,
        1,
    );
    game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        9_u8
    );
}

#[test]
fn crimson_discount_uses_a_real_life_decrease_this_turn() {
    for decrease in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"emitter","card":"unit-follower"}]);
        let mut program = conditional_program(&json!({"crimson":true}), false)
            .card()
            .clone();
        program.abilities.push(ability(
            1,
            vec![instruction(
                1,
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(-1)),
                    },
                },
            )],
        ));
        let mut game = Game::new(
            catalog_with_cost(&json!("3")),
            &initial,
            &Value::Null,
            &Value::Null,
            "crimson-payment",
        )
        .unwrap();
        game.load_internal_card(
            "unit-follower",
            &WellFormedCardProgram::check(program).unwrap(),
        )
        .unwrap();
        if decrease {
            resolved(
                game.activate_internal_card("emitter", AbilityId(1), PlayerId { seat: 1 })
                    .unwrap(),
            );
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
        }
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if decrease { 9_u8 } else { 7_u8 }
        );
    }
}
