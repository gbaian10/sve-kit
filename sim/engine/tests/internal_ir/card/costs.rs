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
    let cards = lowering::ExpandedLowerer::new()
        .unwrap()
        .lower(&tree)
        .unwrap();
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
