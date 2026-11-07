use super::event_cemetery::finish_checks;
use super::*;

fn initial() -> Value {
    json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_u8,"P2":3_u8}},"players":{
        "P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}},
        "P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":10_u8,"max":10_u8},"ep":0_u8,"sep":0_u8,"zones":{}}
    }})
}

#[test]
fn world_break_banishes_both_public_zones_then_each_own_top_ten() {
    for number in ["BP13-107", "BP13-SL23"] {
        let mut initial = initial();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"source","card":number},{"id":"kept-hand","card":"BP01-171"}]);
        for (seat, count) in [("P1", 12_usize), ("P2", 3)] {
            initial["players"][seat]["zones"]["deck"] = json!(
                (0..count)
                    .map(|index| json!({"id":format!("{seat}-deck-{index}"),"card":"BP01-171"}))
                    .collect::<Vec<_>>()
            );
            initial["players"][seat]["zones"]["field"] =
                json!([{"id":format!("{seat}-field"),"card":"BP01-171"}]);
            initial["players"][seat]["zones"]["ex"] =
                json!([{"id":format!("{seat}-ex"),"card":"BP01-171"}]);
            initial["players"][seat]["zones"]["cemetery"] =
                json!([{"id":format!("{seat}-old"),"card":"BP01-171"}]);
        }
        let mut board = game(&initial);
        resolved(
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        let view = board.projection(View::Referee).unwrap();
        for (seat, banished) in [("P1", 12_usize), ("P2", 5)] {
            assert_eq!(view[seat]["field"], json!([]));
            assert_eq!(view[seat]["ex"], json!([]));
            assert_eq!(view[seat]["banish"].as_array().unwrap().len(), banished);
            assert!(
                view[seat]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(format!("{seat}-old")))
            );
        }
        assert_eq!(view["P1"]["deck"], json!(["P1-deck-10", "P1-deck-11"]));
        assert_eq!(view["P2"]["deck"], json!([]));
        assert_eq!(view["P1"]["hand"], json!(["kept-hand"]));
        assert!(
            view["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source"))
        );
        assert_eq!(view["P1"]["pp"]["current"], 2_u8);
        assert!(
            board.projection(View::P2).unwrap()["objects"]
                .get("kept-hand")
                .is_none()
        );
    }
}

#[test]
fn imina_creates_each_players_own_curse_with_the_admitted_body() {
    let mut initial = initial();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP18-103"}]);
    let mut board = game(&initial);
    board
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish_checks(&mut board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("Imina fanfare");
    };
    let handle = options
        .iter()
        .find(|option| option.card == "BP18-103")
        .unwrap()
        .handle;
    resolved(
        board
            .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap(),
    );
    let view = board.projection(View::Referee).unwrap();
    for seat in ["P1", "P2"] {
        let curse = view[seat]["field"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .find(|id| view["objects"][id]["card"] == "BP18-T09")
            .unwrap();
        let known = board.projection(View::P1).unwrap();
        let entry = known["known_cards"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["id"] == curse)
            .unwrap();
        assert_eq!(entry["owner"], seat);
    }
}

#[test]
fn gold_city_counts_both_fields_for_storm_and_recovers_three_pp() {
    for number in ["CP01-004", "CP01-P01"] {
        for total in [7_usize, 8] {
            let mut initial = initial();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] = json!(
                (0..3_usize)
                    .map(|index| json!({"id":format!("mine-{index}"),"card":"BP01-171"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P2"]["zones"]["field"] = json!(
                (0..total - 4)
                    .map(|index| json!({"id":format!("theirs-{index}"),"card":"BP01-171"}))
                    .collect::<Vec<_>>()
            );
            let mut board = game(&initial);
            board
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish_checks(&mut board);
            let question = board.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("Gold City fanfare");
            };
            let handle = options
                .iter()
                .find(|option| option.card == number)
                .unwrap()
                .handle;
            resolved(
                board
                    .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
                    .unwrap(),
            );
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["pp"]["current"], 9_u8);
            assert_eq!(
                view["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                total == 8
            );
            finish_checks(&mut board);
            let outcome = board
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
                )
                .unwrap();
            assert_eq!(outcome.outcome == "resolved", total == 8);
        }
    }
}
