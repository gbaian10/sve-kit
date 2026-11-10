use super::*;
use sve_engine::EngineFailure;

fn input() -> Value {
    let mut document = fanfare::expanded::tree().clone();
    let mut source = document["cards"]["BP01-041"].clone();
    source["abilities"][0]["activate"] = json!({"cost":{"pp":1_u8},"do":[
        {"move":{"top":{"who":"self","n":2_u8},"to":"cemetery","result":"milled"}},
        {"if":{"cond":{"cmp":{"left":{"distinct_orig_costs":{"in":{"objects_of":"milled","view":"before"}}},"op":"eq","right":1_u8}},"then":[{"life":{"who":"self","delta":1_u8}}]}}
    ]});
    document["cards"] = json!({"unit-follower":source});
    document
}

fn game(cards: &[&str]) -> Game {
    let programs = lowerer().lower(&input()).unwrap();
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            row["faces"][0]["cost"] = json!(if row["number"] == "unit-amulet" {
                "3"
            } else {
                "2"
            });
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut position = setup(1, json!([]));
    position["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    position["players"]["P1"]["zones"]["deck"] = json!(
        cards
            .iter()
            .enumerate()
            .map(|(i, card)| json!({"id":format!("d{i}"),"card":card}))
            .collect::<Vec<_>>()
    );
    position["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"old","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog,
        &position,
        &Value::Null,
        &Value::Null,
        "receipt-original-costs",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn actual_milled_members_have_one_distinct_original_cost_only_when_nonempty_and_equal() {
    for (cards, equal) in [
        (vec!["unit-follower", "unit-spell"], true),
        (vec!["unit-follower", "unit-amulet"], false),
        (vec!["unit-spell"], true),
        (vec![], false),
    ] {
        let mut game = game(&cards);
        resolved(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if equal { 21_u8 } else { 20_u8 }
        );
        assert_eq!(
            state["P1"]["cemetery"].as_array().unwrap().len(),
            cards.len().checked_add(1).unwrap()
        );
        assert_eq!(
            state["game"]["ended"], false,
            "zero successful moves do not satisfy equality"
        );
    }
}

#[test]
fn receipt_cost_equality_keeps_other_receipts_projections_filters_and_sources_closed() {
    let mut baseline = input();
    baseline["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1] =
        json!({"life":{"who":"self","delta":1_u8}});
    lowerer().lower(&baseline).unwrap();
    for mutation in 0..6 {
        let mut tree = input();
        let body = &mut tree["cards"]["unit-follower"]["abilities"][0]["activate"]["do"];
        match mutation {
            0 => body[0]["move"]["to"] = json!("ex"),
            1 => body[0]["move"]["top"]["who"] = json!("opp"),
            2 => {
                body[1]["if"]["cond"]["cmp"]["left"]["distinct_orig_costs"]["in"]["view"] =
                    json!("after");
            }
            3 => {
                body[1]["if"]["cond"]["cmp"]["left"]["distinct_orig_costs"]["trait"] =
                    json!("学院");
            }
            4 => {
                body[1]["if"]["cond"]["cmp"]["left"]["distinct_orig_costs"]["in"]["objects_of"] =
                    json!("unbound");
            }
            _ => body.as_array_mut().unwrap().swap(0, 1),
        }
        lowerer().lower(&tree).unwrap_err();
    }
}

#[test]
fn raw_receipt_cost_count_cannot_use_an_ex_producer() {
    let programs = lowerer().lower(&input()).unwrap();
    let mut raw = programs["unit-follower"].card().clone();
    let Node::Instruction { instruction } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("milled receipt producer")
    };
    let Action::Move { movement, .. } = &mut instruction.action else {
        panic!("printed deck movement")
    };
    movement.to = MoveDestination::Owners {
        zone: OwnerZone::Ex,
    };
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut board = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "forged-receipt-cost-producer",
    )
    .unwrap();
    let error = board
        .load_internal_card("unit-follower", &program)
        .unwrap_err();
    assert!(
        matches!(error, EngineFailure::Unsupported(message) if message.contains("deck-to-cemetery producer")),
        "an otherwise valid move to EX cannot authorize the new cost comparison"
    );
}
