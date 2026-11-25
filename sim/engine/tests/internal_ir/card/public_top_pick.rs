use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

fn tree(observation: Value) -> Value {
    let mut steps = json!([
        {"damage":{"to":"enemy","n":{"count":{"in":{"objects_of":"shown","view":"before"},"kind":"follower"}}}},
        {"if":{"cond":{"cmp":{
            "left":{"count":{"in":{"objects_of":"shown","view":"before"}}},"op":"ge","right":1
        }},"then":[
            {"choose":{"bind":"taken","from":{"in":"shown"},"n":1}},
            {"move":{"ref":"taken","to":"hand","result":"entered"}},
            {"move":{"ref":{"in":"shown","except":{"in":{"objects_of":"entered","view":"before"}}},
                "to":"deck","position":"bottom","order":"chooser"}}
        ]}}
    ]);
    steps.as_array_mut().unwrap().insert(0, observation);
    ability(&steps)
}

fn ability(steps: &Value) -> Value {
    equipment_played::tree(
        &json!({"activate":{
            "cost":{"pp":0_u8},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":steps
        }}),
        "unit-amulet",
    )
}

#[test]
fn damage_counts_the_whole_reveal_before_selection_and_orders_only_the_remainder_at_bottom() {
    let program = lowerer()
        .lower(&tree(
            json!({"reveal":{"top":{"who":"self","n":3},"bind":"shown"}}),
        ))
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let ability = program.card().abilities[0].node.id;
    let mut initial = setup(
        1,
        json!([
            {"id":"first","card":"unit-follower"},
            {"id":"picked","card":"unit-spell"},
            {"id":"third","card":"unit-follower"},
            {"id":"tail","card":"unit-spell"}
        ]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"unit-follower"}]);
    let actor = PlayerId { seat: 1 };
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "public-top-pick",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    for printing in ["unit-spell", "unit-follower"] {
        game.load_internal_card(printing, &card(vec![])).unwrap();
    }
    let CardPlay::Awaiting(enemy) = game
        .activate_internal_card("source", ability, actor)
        .unwrap()
    else {
        panic!("formal target");
    };
    let InputDomain::Targets(targets) = &enemy.domain else {
        panic!("formal target domain");
    };
    let CardPlay::Awaiting(pick) = game
        .answer_internal_card(
            actor,
            enemy.token,
            Answer::Targets(vec![targets.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("revealed choice");
    };
    let InputDomain::Targets(choices) = &pick.domain else {
        panic!("revealed choice domain");
    };
    assert_eq!(choices.members.len(), 3);
    let known = game.projection(View::P2).unwrap();
    assert!(
        ["first", "picked", "third"]
            .iter()
            .all(|id| !known["objects"][*id].is_null())
    );
    assert!(known["objects"]["tail"].is_null());
    let after_reveal = game.projection(View::Referee).unwrap();
    assert_eq!(after_reveal["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(
        after_reveal["P1"]["deck"],
        json!(["first", "picked", "third", "tail"])
    );
    let chosen = choices
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("unit-spell"))
        .unwrap();
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(actor, pick.token, Answer::Targets(vec![chosen.handle]))
        .unwrap()
    else {
        panic!("remaining deck order");
    };
    let InputDomain::Order(remaining) = &order.domain else {
        panic!("deck order domain");
    };
    assert_eq!(remaining.len(), 2);
    assert!(game.internal_decision(View::P2).is_none());
    resolved(
        game.answer_internal_card(
            actor,
            order.token,
            Answer::Order(remaining.iter().rev().map(|card| card.handle).collect()),
        )
        .unwrap(),
    );
    let after_order = game.projection(View::Referee).unwrap();
    assert_eq!(after_order["P1"]["deck"], json!(["tail", "third", "first"]));
    assert_eq!(after_order["P1"]["hand"], json!(["picked"]));
    assert!(game.projection(View::P2).unwrap()["objects"]["third"].is_null());
}

#[test]
fn a_private_look_does_not_authorize_the_same_filtered_damage_count() {
    // Only the Look and the damage remain, so the rejection cannot come from the later
    // non-empty check, which separately refuses a private receipt.
    let error = lowerer()
        .lower(&ability(&json!([
            {"look":{"top":{"who":"self","n":3},"bind":"shown"}},
            {"damage":{"to":"enemy","n":{"count":{"in":{"objects_of":"shown","view":"before"},"kind":"follower"}}}}
        ])))
        .unwrap_err();
    assert_eq!(error.code, "unsupported_construct", "{error:?}");
    assert!(error.path.contains("/activate/do/1"), "{error:?}");
}
