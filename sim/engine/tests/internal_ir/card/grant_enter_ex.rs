#![expect(
    clippy::panic,
    reason = "Only the declared hand or field choices and granted pending abilities may occur."
)]
use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};

fn tree(count: u8, name: &str, leave: bool) -> Value {
    let mut steps = vec![
        json!({"grant":{"on":"this","abilities":[{"auto":{"on":{"placed_ex":{"who":"self","sel":{"name":name}}},"do":[{"life":{"who":"self","delta":1}}]}}]}}),
        json!({"choose":{"bind":"moved","from":{"who":"self","zone":"hand"},"n":count}}),
        json!({"move":{"ref":"moved","to":"ex"}}),
    ];
    if leave {
        steps.push(json!({"move":{"ref":"this","to":"cemetery"}}));
    }
    equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":steps}}),
        "unit-amulet",
    )
}

fn board(input: &Value, count: u8, enemy: bool) -> Game {
    let program = lowerer()
        .lower(input)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-amulet"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!(
        (0_u8..count)
            .map(|i| json!({"id":format!("card-{i}"),"card":"unit-spell"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["field"] = if enemy {
        json!([{"id":"opposing","card":"unit-follower"}])
    } else {
        json!([])
    };
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "grant-enter-ex",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    for number in ["unit-spell", "unit-follower"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn activate(game: &mut Game) {
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("declared selection")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("hand or public field choice")
    };
    let answer = Answer::Targets(domain.members.iter().map(|m| m.handle).collect());
    resolved(game.answer_internal_card(q.actor, q.token, answer).unwrap());
    checks(game);
}

fn drain(game: &mut Game) -> usize {
    let mut count = 0_usize;
    while let Some(q) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &q.domain else {
            panic!("granted EX occurrence")
        };
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
        count = count.checked_add(1).unwrap();
        checks(game);
    }
    count
}

#[test]
fn granted_enter_ex_observes_immediate_entries_once_per_card_and_keeps_committed_occurrences() {
    for (number, leave) in [(1_u8, false), (2, false), (2, true)] {
        let mut game = board(&tree(number, "unit-spell", leave), number, false);
        activate(&mut game);
        assert_eq!(drain(&mut game), usize::from(number));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 20_u8 + number);
        assert_eq!(
            view["P1"]["ex"].as_array().unwrap().len(),
            usize::from(number)
        );
        assert_eq!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            leave
        );
    }
}

#[test]
fn granted_enter_ex_ignores_wrong_names_and_the_opponents_ex() {
    let mut wrong = board(&tree(1, "unmatched", false), 1, false);
    activate(&mut wrong);
    assert_eq!(drain(&mut wrong), 0);
    assert_eq!(
        wrong.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        20_u8
    );
    let mut input = tree(1, "unit-follower", false);
    let head = &mut input["cards"]["unit-amulet"]["abilities"][0]["activate"];
    head["targets"] =
        json!([{"bind":"moved","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}]);
    head["do"].as_array_mut().unwrap().remove(1);
    let mut game = board(&input, 0, true);
    activate(&mut game);
    assert_eq!(drain(&mut game), 0);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["ex"], json!(["opposing"]));
    assert_eq!(view["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn granted_enter_ex_keeps_other_event_subject_filters_closed() {
    for on in [
        json!({"placed_ex":{"who":"opp","sel":{"name":"unit-spell"}}}),
        json!({"placed_ex":{"sel":{"name":"unit-spell"}}}),
        json!({"placed_ex":{"who":"self"}}),
        json!({"placed_ex":{"who":"self","sel":{"name":"unit-spell","kind":"spell"}}}),
        json!({"placed_ex":{"who":"self","sel":{"name":"unit-spell"},"during":{"turn":"self"}}}),
    ] {
        let mut input = tree(1, "unit-spell", false);
        input["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"][0]["grant"]["abilities"]
            [0]["auto"]["on"] = on;
        let program = lowerer()
            .lower(&input)
            .unwrap()
            .remove("unit-amulet")
            .unwrap();
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "unadmitted-grant-event",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &program)
            .unwrap_err();
    }
}

#[test]
fn granted_enter_ex_stops_observing_after_the_receiver_leaves() {
    let mut input = tree(1, "unit-spell", false);
    input["cards"]["unit-amulet"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .insert(2, json!({"move":{"ref":"this","to":"cemetery"}}));
    let mut game = board(&input, 1, false);
    activate(&mut game);
    assert_eq!(drain(&mut game), 0);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["leader"]["life"], 20_u8);
    assert_eq!(view["P1"]["ex"], json!(["card-0"]));
    assert_eq!(view["P1"]["cemetery"], json!(["source"]));
}
