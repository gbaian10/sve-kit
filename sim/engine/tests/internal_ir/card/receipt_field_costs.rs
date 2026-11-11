use super::*;
use core::iter::once;
use sve_engine::EngineFailure;

fn input() -> Value {
    let mut document = fanfare::expanded::tree().clone();
    let mut source = document["cards"]["BP01-041"].clone();
    source["abilities"][0]["activate"] = json!({"cost":{"pp":0},"do":[
        {"move":{"ref":{"who":"self","zone":"field","kind":"follower","trait":"機械","orig_cost_ge":6,"other":true},"to":"cemetery","result":"sent"}},
        {"if":{"cond":{"cmp":{"left":{"count":{"in":{"objects_of":"sent","view":"before"},"orig_cost_eq":6}},"op":"gt","right":0}},"then":[{"life":{"who":"self","delta":1}}]}},
        {"if":{"cond":{"cmp":{"left":{"count":{"in":{"objects_of":"sent","view":"before"},"orig_cost_eq":7}},"op":"gt","right":0}},"then":[{"damage":{"to":{"leaders":"opponents"},"n":4}}]}}
    ]});
    document["cards"] = json!({"unit-follower":source});
    document
}

fn printed(token: bool) -> Arc<Catalog> {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            let (cost, kind) = match row["number"].as_str().unwrap() {
                "unit-follower" => ("6", "フォロワー"),
                "unit-spell" => (
                    "7",
                    if token {
                        "フォロワー・トークン"
                    } else {
                        "フォロワー"
                    },
                ),
                _ => ("5", "フォロワー"),
            };
            row["faces"][0]["cost"] = json!(cost);
            row["faces"][0]["card_type"] = json!(kind);
            row["faces"][0]["power"] = json!("2");
            row["faces"][0]["hp"] = json!("8");
            row["faces"][0]["traits"] = json!(["機械"]);
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    )
}

fn board(members: &[&str], token: bool, body: Option<&Value>) -> Game {
    let mut document = input();
    if let Some(body) = body {
        document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = body.clone();
    }
    let program = lowerer().lower(&document).unwrap()["unit-follower"].clone();
    let mut position = setup(1, json!([]));
    position["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut field = vec![json!({"id":"s","card":"unit-follower"})];
    field.extend(members.iter().enumerate().map(|(index, card)| json!({"id":format!("m{index}"),"card":card,"state":{"cost":if *card=="unit-amulet" {99} else {0}}})));
    position["players"]["P1"]["zones"]["field"] = json!(field);
    position["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"old6","card":"unit-follower"},{"id":"old7","card":"unit-spell"}
    ]);
    let mut game = Game::new(
        printed(token),
        &position,
        &Value::Null,
        &Value::Null,
        "field-receipt-original-cost",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn field_receipt_original_cost_branches_use_only_this_movements_successes() {
    for (members, six, seven) in [
        (vec![], false, false),
        (vec!["unit-amulet"], false, false),
        (vec!["unit-follower"], true, false),
        (vec!["unit-spell"], false, true),
        (
            vec!["unit-follower", "unit-spell", "unit-amulet"],
            true,
            true,
        ),
    ] {
        let mut game = board(&members, false, None);
        resolved(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], if six { 21 } else { 20 });
        assert_eq!(state["P2"]["leader"]["life"], if seven { 16 } else { 20 });
        let expected: Vec<_> = members
            .iter()
            .enumerate()
            .filter(|(_, card)| **card == "unit-amulet")
            .map(|(index, _)| json!(format!("m{index}")))
            .chain(once(json!("s")))
            .collect();
        let field = state["P1"]["field"].as_array().unwrap();
        assert_eq!(
            field.len(),
            expected.len(),
            "printed five stays despite its modified cost of 99; the source is excluded"
        );
        for id in expected {
            assert!(field.contains(&id));
        }
    }
}

#[test]
fn a_field_receipt_retains_original_printing_after_token_erasure() {
    let mut game = board(&["unit-spell"], true, None);
    resolved(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["leader"]["life"], 16,
        "the original seven is still readable after erasure"
    );
    assert!(
        !state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("m0")),
        "a departing follower token does not stay in the cemetery"
    );
}

#[test]
fn author_field_receipts_do_not_grant_unretained_characteristics_or_other_destinations() {
    for change in 0..5 {
        let mut document = input();
        let body = &mut document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"];
        match change {
            0 => body[0]["move"]["to"] = json!("hand"),
            1 => body[0]["move"]["ref"]["who"] = json!("opp"),
            2 => body[1]["if"]["cond"]["cmp"]["left"]["count"]["in"]["view"] = json!("after"),
            3 => body[1]["if"]["cond"]["cmp"]["left"]["count"]["trait"] = json!("機械"),
            _ => {
                body[1]["if"]["cond"]["cmp"]["left"]["count"]["in"]["objects_of"] =
                    json!("foreign");
            }
        }
        lowerer().lower(&document).unwrap_err();
    }
}

#[test]
fn raw_field_receipts_need_the_checked_source_destination_and_original_before_view() {
    let original = lowerer().lower(&input()).unwrap()["unit-follower"]
        .card()
        .clone();
    for change in 0..4 {
        let mut raw = original.clone();
        if change < 3 {
            let Node::Instruction { instruction } =
                &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
            else {
                panic!("move")
            };
            let Action::Move {
                movement, objects, ..
            } = &mut instruction.action
            else {
                panic!("move")
            };
            if change == 0 {
                movement.to = MoveDestination::Owners {
                    zone: OwnerZone::Hand,
                };
            } else {
                let Selector::Difference { input, .. } = objects else {
                    panic!("other")
                };
                let Selector::Zone { players, zone, .. } = input.as_mut() else {
                    panic!("field")
                };
                if change == 1 {
                    players.relation = PlayerRelation::Opponents;
                } else {
                    *zone = ZoneKind::Ex;
                }
            }
        } else {
            let Node::Control {
                control:
                    Control::If {
                        condition: Expr::Compute(predicate),
                        ..
                    },
            } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[1].node
            else {
                panic!("condition")
            };
            let Predicate::ReceiptAny { projection, .. } = predicate.as_mut() else {
                panic!("receipt")
            };
            *projection = MemberProjection::After;
        }
        if change == 3 {
            WellFormedCardProgram::check(raw).unwrap_err();
            continue;
        }
        let program = WellFormedCardProgram::check(raw).unwrap();
        let mut game = Game::new(
            printed(false),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "raw-field-receipt",
        )
        .unwrap();
        let error = game
            .load_internal_card("unit-follower", &program)
            .unwrap_err();
        assert!(
            matches!(error, EngineFailure::Unsupported(_)),
            "a valid raw program still needs the matching backend producer authority"
        );
    }
}

#[test]
fn field_movement_success_count_and_did_share_the_actual_successful_members() {
    let movement = input()["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0].clone();
    let body = json!([
        movement,
        {"damage":{"to":{"leaders":"opponents"},"n":{"times":[{"count_of":"sent"},2]}}},
        {"if":{"cond":{"did":"sent"},"then":[{"life":{"who":"self","delta":1}}]}}
    ]);
    for (members, count) in [
        (vec!["unit-amulet"], 0_i64),
        (vec!["unit-follower"], 1_i64),
        (vec!["unit-follower", "unit-spell", "unit-amulet"], 2_i64),
    ] {
        let mut game = board(&members, false, Some(&body));
        resolved(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            20_i64 - count * 2,
            "count_of supplies the successful movement count to arithmetic"
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            20_i64 + i64::from(count > 0),
            "did sees no success for an empty result"
        );
    }
}
