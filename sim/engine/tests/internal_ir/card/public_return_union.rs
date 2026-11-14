use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(retire: bool) -> Value {
    let mut steps = vec![];
    if retire {
        steps.push(json!({"banish":"picked"}));
    }
    steps.extend([
        json!({"move":{"ref":"picked","to":"deck"}}),
        json!({"shuffle":{"who":"self"}}),
    ]);
    equipment_played::tree(
        &json!({"activate":{
            "cost":{"pp":0},
            "targets":[{"bind":"picked","from":{"who":"self","zone":["cemetery","banish"]},"upto":2}],
            "do":steps
        }}),
        "unit-amulet",
    )
}

fn position(retire: bool) -> (Game, AbilityId) {
    let program = lowerer()
        .lower(&tree(retire))
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let ability = program.card().abilities[0].node.id;
    let mut initial = setup(1, json!([{"id":"d0","card":"unit-follower"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"grave","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["banish"] = json!([
        {"id":"out","card":"unit-follower"},{"id":"hidden","card":"unit-follower","face_up":false}
    ]);
    initial["players"]["P2"]["zones"]["cemetery"] =
        json!([{"id":"foreign","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "public-union",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    (game, ability)
}

#[test]
fn a_public_return_union_excludes_hidden_and_foreign_cards_and_moves_both_zones() {
    let (mut game, ability) = position(false);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("formal targets")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("public targets")
    };
    assert_eq!(domain.members.len(), 2);
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 1]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert_eq!(state["P1"]["banish"], json!(["hidden"]));
    let deck = state["P1"]["deck"].as_array().unwrap();
    assert_eq!(deck.len(), 3);
    for id in ["d0", "grave", "out"] {
        assert!(deck.contains(&json!(id)));
    }
    assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
}

#[test]
fn returning_an_old_generation_does_not_recapture_it_from_the_other_union_zone() {
    let (mut game, ability) = position(true);
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("formal targets")
    };
    let outcome = resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["deck"], json!(["d0"]));
    assert!(
        state["P1"]["banish"]
            .as_array()
            .unwrap()
            .contains(&json!("grave"))
    );
    assert!(
        outcome
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Shuffled { .. }))
    );
}

#[test]
fn public_return_unions_reject_other_players_and_unrelated_zone_combinations() {
    lowerer().lower(&tree(false)).unwrap();
    let mut reversed = tree(false);
    reversed["cards"]["unit-amulet"]["abilities"][0]["activate"]["targets"][0]["from"]["zone"] =
        json!(["banish", "cemetery"]);
    lowerer().lower(&reversed).unwrap();
    for from in [
        json!({"who":"opp","zone":["cemetery","banish"]}),
        json!({"who":"all","zone":["cemetery","banish"]}),
        json!({"who":"self","zone":["cemetery","ex"]}),
        json!({"who":"self","zone":["cemetery","banish","field"]}),
        json!({"who":"self","zone":["banish","banish"]}),
    ] {
        let mut invalid = tree(false);
        invalid["cards"]["unit-amulet"]["abilities"][0]["activate"]["targets"][0]["from"] = from;
        lowerer().lower(&invalid).unwrap_err();
    }
}

#[test]
fn named_cemetery_population_moves_to_ex_through_ordinary_capacity() {
    let document = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[
            {"move":{"ref":{"who":"self","zone":"cemetery","name":"unit-follower"},"to":"ex"}}
        ]}}),
        "unit-amulet",
    );
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"g0","card":"unit-follower"},{"id":"g1","card":"unit-follower"},{"id":"other","card":"unit-spell"}
    ]);
    initial["players"]["P2"]["zones"]["cemetery"] =
        json!([{"id":"foreign","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "public-return",
    )
    .unwrap();
    for (number, program) in [
        ("unit-amulet", program.clone()),
        ("unit-follower", card(vec![])),
        ("unit-spell", card(vec![])),
    ] {
        game.load_internal_card(number, &program).unwrap();
    }
    resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["g0", "g1"]));
    assert_eq!(state["P1"]["cemetery"], json!(["other"]));
    assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
}

#[test]
fn public_moves_exclude_the_ability_holder_and_preserve_other_players() {
    let document = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[
            {"move":{"ref":{"who":"self","zone":"field","kind":"follower","evolved":false,"other":true},"to":"ex","result":"returned"}}
        ]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower"},{"id":"ally","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "public-move-other",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["s"]));
    assert_eq!(state["P1"]["ex"], json!(["ally"]));
    assert_eq!(state["P2"]["field"], json!(["f"]));
}

#[test]
fn public_move_receipts_use_the_same_destination_authority_as_object_moves() {
    for (who, zone, to, accepted) in [
        ("opp", "cemetery", "hand", false),
        ("opp", "field", "cemetery", false),
        ("self", "cemetery", "ex", true),
    ] {
        let query = if zone == "field" {
            json!({"who":who,"zone":zone,"evolved":false})
        } else {
            json!({"who":who,"zone":zone,"name":"unit-follower"})
        };
        let document = equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"do":[
                {"move":{"ref":query,"to":to,"result":"returned"}}
            ]}}),
            "unit-follower",
        );
        let result = lowerer().lower(&document);
        assert_eq!(result.is_ok(), accepted, "{who} {zone} to {to}: {result:?}");
    }
}
