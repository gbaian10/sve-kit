use super::*;
use sve_engine::game::internal::{Answer, InputDomain};

fn lowered(activate: Value) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = activate;
    tree["cards"] = json!({"unit-follower":seed});
    lowerer().lower(&tree).unwrap()["unit-follower"].clone()
}

#[test]
fn source_and_chosen_hand_costs_reserve_distinct_cards_before_payment() {
    let program = lowered(json!({
        "zone":"hand",
        "cost":{"steps":[{"pp":1},{"discard":{"ref":"this"}},
            {"discard":{"sel":{"who":"self","zone":"hand"},"n":1}}]},
        "do":[{"life":{"who":"self","delta":1}}]
    }));
    for extra in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = if extra {
            json!([{"id":"s","card":"unit-follower"},{"id":"material","card":"unit-spell"}])
        } else {
            json!([{"id":"s","card":"unit-follower"}])
        };
        let mut board =
            Game::new(catalog(), &initial, &Value::Null, &Value::Null, "hand-cost").unwrap();
        board.load_internal_card("unit-follower", &program).unwrap();
        board
            .load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let before = exact_authority(&board);
        let play = board.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 });
        if !extra {
            play.unwrap_err();
            assert_eq!(exact_authority(&board), before);
            continue;
        }
        let CardPlay::Awaiting(question) = play.unwrap() else {
            panic!("hand materials")
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("cost domain")
        };
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.members[0].card.as_deref(), Some("unit-spell"));
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            2
        );
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap(),
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"], json!(["s", "material"]));
        assert_eq!(state["P1"]["pp"]["current"], 1);
    }
}

#[test]
fn cemetery_quick_source_banish_is_quoted_without_moving_it() {
    let program = lowered(json!({"zone":"cemetery","quick":true,
        "cost":{"banish_self":true},"do":[{"life":{"who":"self","delta":1}}]}));
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["cemetery"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut board = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "cemetery-quick",
    )
    .unwrap();
    board.load_internal_card("unit-follower", &program).unwrap();
    board
        .load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = board.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].zone, "cemetery");
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["s"])
    );
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(options[0].handle)),
            )
            .unwrap(),
    );
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["banish"],
        json!(["s"])
    );
}

#[test]
fn quick_ability_with_public_material_costs_still_rejects_at_admission() {
    let program = lowered(json!({"zone":"cemetery","quick":true,
        "cost":{"banish":{"sel":{"who":"self","zone":"cemetery","name":"unit-spell"},"n":1}},
        "do":[{"life":{"who":"self","delta":1}}]}));
    let mut board = game(1, 1);
    let before = exact_authority(&board);
    assert!(board.load_internal_card("unit-follower", &program).is_err());
    assert_eq!(exact_authority(&board), before);
}
