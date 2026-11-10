#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the complete Ginger sequence."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn pending_ginger(board: &mut Game, number: &str) {
    finish_checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("Ginger fanfare")
    };
    assert_eq!(
        options.len(),
        1,
        "only Ginger's committed fanfare is pending"
    );
    assert_eq!(
        options[0].card, number,
        "the actual printing owns the fanfare"
    );
    resolved(
        board
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    finish_checks(board);
}

#[test]
fn r7b_ginger_all_prints_discount_from_own_banish_create_and_play_the_complete_token() {
    for number in ["BP18-042", "BP18-SL12", "PCS02-002"] {
        assert!(selection_with_rules().current.contains_key(number));
        assert!(selection_with_rules().current.contains_key("BP18-T04"));
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!("ウィッチ");
        initial["players"]["P2"]["leader"]["class"] = json!("エルフ");
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"ginger","card":number}]);
        initial["players"]["P1"]["zones"]["banish"] = json!(
            (0_u8..3)
                .map(|i| json!({"id":format!("own-banish{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["banish"] = json!(
            (0_u8..2)
                .map(|i| json!({"id":format!("opponent-banish{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-002","state":{"power":9_u8,"hp":6_u8,"max_hp":9_u8,"damage":3_u8}}
        ]);
        let mut board = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "r7b-ginger",
        )
        .unwrap();
        let before = board.projection(View::Referee).unwrap();
        assert!(
            before["objects"]["enemy"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
        assert!(matches!(
            board
                .play_internal_card("ginger", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        pending_ginger(&mut board, number);
        let created = board.projection(View::Referee).unwrap();
        assert_eq!(created["P1"]["pp"]["current"], 3_u8);
        let ex = created["P1"]["ex"].as_array().unwrap();
        assert_eq!(ex.len(), 1);
        let token = ex[0].as_str().unwrap();
        let CardPlay::Awaiting(q) = board
            .play_internal_card(token, PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("one follower target")
        };
        let InputDomain::Targets(domain) = q.domain else {
            panic!("follower target domain")
        };
        assert_eq!(domain.members.len(), 1);
        resolved(
            board
                .answer_internal_card(
                    q.actor,
                    q.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        finish_checks(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 2_u8);
        assert_eq!(after["objects"]["enemy"]["power"], 1_u8);
        assert_eq!(after["objects"]["enemy"]["hp"], 1_u8);
        assert_eq!(after["objects"]["enemy"]["keywords"], json!([]));
        assert_eq!(after["objects"]["enemy"]["boxed"], false);
        assert_eq!(
            after["objects"]["enemy"]["traits"],
            before["objects"]["enemy"]["traits"]
        );
        assert_eq!(after["P1"]["ex"], json!([]));
    }
}
