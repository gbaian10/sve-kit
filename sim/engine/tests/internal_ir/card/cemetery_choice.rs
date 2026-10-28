use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(query: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":0},"do":[
        {"choose":{"bind":"returned","from":query,"n":1}},
        {"move":{"ref":"returned","to":"hand"}}
    ]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

#[test]
fn cemetery_choice_can_return_a_spell_without_selecting_a_field_follower() {
    let programs = lowerer()
        .lower(&tree(
            &json!({"who":"self","zone":"cemetery","kind":"spell"}),
        ))
        .unwrap();
    let program = &programs["unit-follower"];
    let ability = program.card().abilities[0].node.id;
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"found","card":"unit-spell"},{"id":"ignored","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "cemetery-choice",
    )
    .unwrap();
    game.load_internal_card("unit-follower", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("expected grave choice")
    };
    assert!(
        matches!(&question.domain, InputDomain::Targets(domain) if domain.members.len() == 1 && domain.members[0].card.as_deref() == Some("unit-spell"))
    );
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["cemetery"], json!(["ignored"]));
    assert_eq!(state["P1"]["field"], json!(["s"]));
}

#[test]
fn cemetery_choice_keeps_other_owners_and_private_sources_closed() {
    for query in [
        json!({"who":"self","zone":"hand"}),
        json!({"who":"opp","zone":"cemetery"}),
        json!({"who":"all","zone":"cemetery"}),
        json!({"who":"self","zone":"cemetery","kind":"follower","leader_or":true}),
    ] {
        lowerer().lower(&tree(&query)).unwrap_err();
    }
}
