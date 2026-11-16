use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{pending, question};
use super::*;

fn initial() -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
    value["players"]["P2"]["leader"]["class"] = json!("ウィッチ");
    value["players"]["P1"]["zones"]["hand"] = json!([]);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"ruzen","card":"BP10-076"}]);
    value
}

fn loaded(value: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        value,
        &Value::Null,
        &Value::Null,
        "ruzen",
    )
    .unwrap()
}

#[test]
fn ruzen_fanfare_lets_each_holder_discard_their_own_hand_to_two() {
    let mut value = initial();
    value["players"]["P1"]["zones"]["field"] = json!([]);
    value["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"ruzen","card":"BP10-076"},{"id":"m1","card":"BP01-037"},
        {"id":"m2","card":"BP01-037"},{"id":"m3","card":"BP01-037"}
    ]);
    value["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"t1","card":"BP01-037"},{"id":"t2","card":"BP01-037"},{"id":"t3","card":"BP01-037"}
    ]);
    let mut board = loaded(&value);
    assert!(matches!(
        board
            .play_internal_card("ruzen", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let first = question(pending(&mut board, "BP10-076"));
    assert_eq!(first.actor, PlayerId { seat: 1 });
    let InputDomain::Targets(own) = &first.domain else {
        panic!("first discard")
    };
    assert_eq!(
        own.cardinality,
        Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1))
        }
    );
    let second = question(
        board
            .answer_internal_card(
                first.actor,
                first.token,
                Answer::Targets(vec![own.members[0].handle]),
            )
            .unwrap(),
    );
    assert_eq!(second.actor, PlayerId { seat: 2 });
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let InputDomain::Targets(other) = &second.domain else {
        panic!("second discard")
    };
    resolved(
        board
            .answer_internal_card(
                second.actor,
                second.token,
                Answer::Targets(vec![other.members[0].handle]),
            )
            .unwrap(),
    );
    finish_checks(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn ruzen_blocks_craigs_draw_but_not_his_discard_or_the_play_of_insight_even_on_an_empty_deck() {
    for empty in [false, true] {
        let mut value = initial();
        value["turn"]["active"] = json!("P2");
        value["players"]["P2"]["zones"]["hand"] = json!([
            {"id":"craig","card":"BP02-046"},{"id":"insight","card":"SD03-005"},{"id":"fee","card":"BP01-037"}
        ]);
        value["players"]["P2"]["zones"]["deck"] = if empty {
            json!([])
        } else {
            json!([{"id":"d","card":"BP01-037"}])
        };
        let mut board = loaded(&value);
        assert!(matches!(
            board
                .play_internal_card("craig", PlayerId { seat: 2 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let q = question(pending(&mut board, "BP02-046"));
        assert_eq!(q.actor, PlayerId { seat: 2 });
        let InputDomain::Targets(domain) = &q.domain else {
            panic!("Craig still discards")
        };
        let fee = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("BP01-037"))
            .unwrap()
            .handle;
        resolved(
            board
                .answer_internal_card(q.actor, q.token, Answer::Targets(vec![fee]))
                .unwrap(),
        );
        finish_checks(&mut board);
        resolved(
            board
                .play_internal_card("insight", PlayerId { seat: 2 })
                .unwrap(),
        );
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["hand"], json!([]));
        assert_eq!(
            after["P2"]["deck"].as_array().unwrap().len(),
            usize::from(!empty)
        );
        assert!(after["game"]["winner"].is_null());
    }
}

#[test]
fn ruzens_damage_reduction_stacks_and_still_applies_before_lethal_damage_is_checked() {
    for copies in [1_u8, 2] {
        let mut value = initial();
        value["turn"]["active"] = json!("P2");
        value["players"]["P2"]["zones"]["hand"] = json!([{"id":"dance","card":"BP02-120"}]);
        if copies == 2 {
            value["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"second","card":"BP10-SL20"}));
        }
        let mut board = loaded(&value);
        let mut play = board
            .play_internal_card("dance", PlayerId { seat: 2 })
            .unwrap();
        while let CardPlay::Awaiting(q) = play {
            let answer = if let InputDomain::Targets(domain) = &q.domain {
                Answer::Targets(vec![domain.members[0].handle])
            } else {
                let InputDomain::DamageReplacements(options) = &q.domain else {
                    panic!("Death's Dance question");
                };
                assert_eq!(q.actor, PlayerId { seat: 1 });
                Answer::Order(options.iter().map(|option| option.handle).collect())
            };
            play = board
                .answer_internal_card(q.actor, q.token, answer)
                .unwrap();
        }
        resolved(play);
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 18_u8 + copies);
        assert!(
            after["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("ruzen"))
        );
    }
}

#[test]
fn ruzen_allows_the_opponents_normal_turn_start_draw() {
    let mut value = initial();
    value["players"]["P2"]["zones"]["hand"] = json!([]);
    value["players"]["P2"]["zones"]["deck"] = json!([
        {"id":"start-card","card":"BP01-037"},{"id":"later","card":"BP01-037"}
    ]);
    let mut board = loaded(&value);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    finish_checks(&mut board);
    if board.projection(View::Referee).unwrap()["turn"]["active"] == "P1" {
        if let Some(q) = board.internal_quick_choice().unwrap() {
            board
                .answer_internal_card(q.actor, q.token, Answer::Quick(None))
                .unwrap();
            finish_checks(&mut board);
        }
        if board.projection(View::Referee).unwrap()["turn"]["active"] == "P1" {
            board
                .internal_rule_action(&json!({"do":"end-discard","select":[]}))
                .unwrap();
            finish_checks(&mut board);
        }
    }
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["turn"]["active"], "P2");
    assert_eq!(after["P2"]["hand"], json!(["start-card"]));
    assert_eq!(after["P2"]["deck"], json!(["later"]));
}
