use super::*;
use sve_engine::game::internal::DecisionView;

#[expect(
    clippy::panic,
    reason = "Unexpected fixture questions fail the scenario."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected declaration or target");
    };
    question
}

#[test]
fn bp04_066_declares_extra_pp_before_choosing_target_and_only_then_pays() {
    let (snapshot, keywords, input) = inputs();
    let selection = Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap();
    for (accepted, pp, quick) in [
        (false, 10_u8, false),
        (true, 10, false),
        (true, 9, false),
        (true, 10, true),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP04-066"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy1","card":"BP01-037"},{"id":"enemy2","card":"BP01-037"}
        ]);
        if quick {
            initial["turn"]["active"] = json!("P2");
        }
        let mut game = Game::from_selected_effects(
            selection.clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "optional-payment",
        )
        .unwrap();
        let original = game.projection(View::Referee).unwrap();
        let declare = if quick {
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            let window = game.internal_quick_choice().unwrap().unwrap();
            let InputDomain::Quick { options, .. } = &window.domain else {
                panic!("Quick window");
            };
            assert_eq!(options.len(), 1);
            awaiting(
                game.answer_internal_card(window.actor, window.token, Answer::Quick(Some(0)))
                    .unwrap(),
            )
        } else {
            awaiting(
                game.play_internal_card("spell", PlayerId { seat: 1 })
                    .unwrap(),
            )
        };
        assert_eq!(declare.domain, InputDomain::OptionalCost);
        let target = awaiting(
            game.answer_internal_card(declare.actor, declare.token, Answer::OptionalCost(accepted))
                .unwrap(),
        );
        assert!(matches!(target.domain, InputDomain::Targets(_)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            pp
        );
        let before = format!("{game:?}");
        game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![]))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "invalid targets preserve authority"
        );
        let played =
            game.answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]));
        if pp < 10 && accepted {
            played.unwrap_err();
            assert!(
                game.projection(View::Referee).unwrap() == original,
                "failed surcharge restores hand, PP, events and public state"
            );
            assert!(game.internal_decision(View::Referee).is_none());
            continue;
        }
        resolved(played.unwrap());
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
