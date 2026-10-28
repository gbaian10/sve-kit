use super::*;

#[test]
fn bp20_p60_activates_in_ex_once_per_turn_and_preserves_damage() {
    for seat in ["P1", "P2"] {
        let other = if seat == "P1" { "P2" } else { "P1" };
        let actor = PlayerId {
            seat: seat_number(seat),
        };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["ex"] = json!([{"id":"crest","card":"BP20-P60"}]);
        initial["players"][other]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"damage":2_u8}}]);
        let mut board = game(&initial);
        let CardPlay::Awaiting(question) = board
            .activate_internal_card("crest", AbilityId(1), actor)
            .unwrap()
        else {
            panic!("target");
        };
        resolved(
            board
                .answer_internal_card(actor, question.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 1_i64);
        assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
        let before = format!("{board:?}");
        board
            .activate_internal_card("crest", AbilityId(1), actor)
            .unwrap_err();
        assert_eq!(format!("{board:?}"), before);
        for zone in ["field", "hand", "cemetery"] {
            let mut invalid = initial.clone();
            invalid["players"][seat]["zones"][zone] =
                invalid["players"][seat]["zones"]["ex"].take();
            let mut invalid_board = game(&invalid);
            let invalid_before = format!("{invalid_board:?}");
            invalid_board
                .activate_internal_card("crest", AbilityId(1), actor)
                .unwrap_err();
            assert_eq!(format!("{invalid_board:?}"), invalid_before);
        }
    }
}
