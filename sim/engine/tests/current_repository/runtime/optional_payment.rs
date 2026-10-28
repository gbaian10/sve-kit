use super::*;

#[test]
fn bp04_066_declares_extra_pp_before_choosing_target_and_only_then_pays() {
    for accepted in [false, true] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP04-066"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy1","card":"BP01-037"},{"id":"enemy2","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        let CardPlay::Awaiting(declare) = game
            .play_internal_card("spell", PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("optional declaration");
        };
        assert_eq!(declare.domain, InputDomain::OptionalCost);
        let CardPlay::Awaiting(target) = game
            .answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(accepted))
            .unwrap()
        else {
            panic!("target still required after accepting surcharge");
        };
        assert!(matches!(target.domain, InputDomain::Targets(_)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            10_u8
        );
        let before = format!("{game:?}");
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![]))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "invalid targets preserve authority"
        );
        resolved(
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if accepted { 0_u8 } else { 5 }
        );
        assert_eq!(
            state["P2"]["field"],
            if accepted {
                json!([])
            } else {
                json!(["enemy2"])
            }
        );
        assert_eq!(
            state["P2"]["banish"],
            if accepted {
                json!(["enemy1", "enemy2"])
            } else {
                json!(["enemy1"])
            }
        );
    }
}
