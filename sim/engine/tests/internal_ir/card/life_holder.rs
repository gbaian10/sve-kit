use super::*;

fn tree(body: &Value) -> Value {
    let mut expanded = fanfare::expanded::tree().clone();
    expanded["cards"]["BP08-038"]["abilities"][0]["fanfare"] = body.clone();
    expanded["cards"] = json!({"unit-follower":expanded["cards"]["BP08-038"]});
    expanded
}

fn fanfare_follower(body: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(&tree(body))
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

fn rejected(body: &Value) {
    lowerer().lower(&tree(body)).unwrap_err();
}

fn play_fanfare(initial: &Value, body: &Value) -> Value {
    let mut game = Game::new(
        catalog(),
        initial,
        &Value::Null,
        &Value::Null,
        "life-holder",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &fanfare_follower(body))
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    assert!(
        matches!(
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ),
        "the follower enters before its fanfare resolves"
    );
    if let Some(pending) = game.internal_pending_choice().unwrap() {
        let mut step = game
            .answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
            .unwrap();
        while let CardPlay::Awaiting(question) = step {
            step = game
                .answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
                .unwrap();
        }
    }
    game.projection(View::Referee).unwrap()
}

fn position() -> Value {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P2"]["leader"]["life"] = json!(12_u8);
    initial
}

#[test]
fn master_of_heals_the_chosen_followers_controller_after_it_is_destroyed() {
    let body = json!({
        "targets":[{"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
        "do":[{"destroy":"victim"},{"life":{"who":{"master_of":"victim"},"delta":3}}]
    });
    let mut initial = position();
    // P1 owns the follower P2 controls: 「それのリーダー」 is the controller's leader.
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"unit-follower","owner":"P1"}]);
    let view = play_fanfare(&initial, &body);
    assert_eq!(
        view["P1"]["cemetery"],
        json!(["enemy"]),
        "the owner's cemetery receives it"
    );
    assert_eq!(
        view["P2"]["leader"]["life"], 15_u8,
        "its controller's leader heals"
    );
    assert_eq!(view["P1"]["leader"]["life"], 10_u8);
}

#[test]
fn life_holders_stay_closed_where_no_card_needs_them() {
    let target =
        json!([{"bind":"victim","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}]);
    for body in [
        json!({"do":[{"life":{"who":"opp","delta":3}}]}),
        json!({"do":[{"life":{"who":"opponents","delta":3}}]}),
        json!({"targets":target,"do":[{"set_life":{"who":{"master_of":"victim"},"n":6}}]}),
        json!({"do":[{"set_life":{"who":"opponents","n":6}}]}),
        json!({"do":[{"life":{"who":{"master_of":"unbound"},"delta":3}}]}),
    ] {
        rejected(&body);
    }
}
