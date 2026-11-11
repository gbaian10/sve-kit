use super::*;
use alloc::collections::BTreeSet;

fn tree() -> Value {
    equipment_played::tree(
        &json!({"static":{"effects":[
            {"if":{"cond":{"awakened":true},"then":[{"grant":{"keywords":["storm"]}}]}},
            {"if":{"cond":{"leader_life":{"who":"self","le":19}},"then":[{"grant":{"keywords":["bane"]}}]}}
        ]}}),
        "unit-follower",
    )
}

fn program(tree: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn board(card: &WellFormedCardProgram, maximum: u8, life: u8, active: &str) -> Game {
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!(active);
    initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
    initial["players"]["P1"]["leader"]["life"] = json!(life);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["hand"] = json!([]);
    }
    let mut game = Game::new(
        keyword_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "per-grant-condition",
    )
    .unwrap();
    game.load_internal_card("unit-follower", card).unwrap();
    game
}

fn keywords(game: &Game) -> BTreeSet<String> {
    serde_json::from_value::<Vec<String>>(
        game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"].clone(),
    )
    .unwrap()
    .into_iter()
    .collect()
}

#[test]
fn each_grant_condition_is_independent_and_conjoined_with_the_ability_condition() {
    for head in [false, true] {
        let mut input = tree();
        if head {
            input["cards"]["unit-follower"]["abilities"][0]["static"]["while"] =
                json!({"my_turn":true});
        }
        let card = program(&input);
        assert_eq!(
            card.card().abilities.len(),
            1,
            "one source ability is retained"
        );
        let ability = &card.card().abilities[0].node;
        assert_eq!(
            ability.declarations.len(),
            2,
            "each conditional Grant stays one declaration"
        );
        for effect in &ability.declarations {
            let Effect::Continuous { source, .. } = effect else {
                panic!("Grant declaration")
            };
            assert_eq!(
                source, &ability.source,
                "both declarations retain the same origin"
            );
        }
        for (maximum, life, expected) in [
            (6_u8, 20_u8, vec![]),
            (7, 20, vec!["疾走"]),
            (6, 19, vec!["必殺"]),
            (7, 19, vec!["疾走", "必殺"]),
        ] {
            for active in ["P1", "P2"] {
                let game = board(&card, maximum, life, active);
                let expected: BTreeSet<String> = if head && active == "P2" {
                    BTreeSet::new()
                } else {
                    expected
                        .iter()
                        .map(|keyword| (*keyword).to_owned())
                        .collect()
                };
                assert_eq!(keywords(&game), expected);
            }
        }
    }
}

#[test]
fn conditional_grants_reject_private_reads_in_author_and_raw_ir() {
    let mut input = tree();
    input["cards"]["unit-follower"]["abilities"][0]["static"]["effects"][0]["if"]["cond"] = json!({
        "cmp":{"left":{"count":{"who":"opp","zone":"hand","name":"private-card"}},"op":"ge","right":1}
    });
    lowerer().lower(&input).unwrap_err();

    let mut raw = program(&tree()).card().clone();
    let binding = Binding::new(99, raw.abilities[0].node.scope);
    let mut objects = zone(ZoneKind::Hand, 99);
    let Selector::Zone {
        binding: candidate,
        players: audience,
        predicate,
        ..
    } = &mut objects
    else {
        panic!("private hand query")
    };
    *candidate = binding;
    *audience = players(PlayerRelation::Opponents);
    **predicate = Expr::Compute(Box::new(Predicate::NamesIntersect {
        left: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::Bound { binding }),
            },
        })),
        right: Expr::Literal(vec![CardName("private-card".into())]),
    }));
    let Effect::Continuous { condition, .. } = &mut raw.abilities[0].node.declarations[0] else {
        panic!("conditional Grant")
    };
    *condition = Some(Expr::Compute(Box::new(Predicate::Exists { objects })));
    let checked = WellFormedCardProgram::check(raw).unwrap();
    let mut game = Game::new(
        keyword_catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "private-static-grant",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &checked)
        .unwrap_err();
}

#[test]
fn conditional_grants_keep_other_families_and_cross_zone_conditions_closed() {
    let mut mixed = tree();
    mixed["cards"]["unit-follower"]["abilities"][0]["static"]["effects"][1] =
        json!({"cost_mod":{"delta":-1}});
    lowerer().lower(&mixed).unwrap_err();
    for zones in [json!("equipment"), json!(["field", "ex"])] {
        let mut cross = tree();
        cross["cards"]["unit-follower"]["abilities"][0]["static"]["zone"] = zones;
        lowerer().lower(&cross).unwrap_err();
    }
}
