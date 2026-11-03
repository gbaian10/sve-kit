use super::*;
use sve_engine::game::internal::DecisionView;

#[expect(
    clippy::panic,
    reason = "Unexpected fixture questions invalidate the scenario."
)]
fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("hand selection");
    };
    question
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture questions invalidate the scenario."
)]
fn trigger(game: &mut Game) -> DecisionView {
    if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("trigger");
    };
    assert_eq!(
        options.len(),
        1,
        "the fixture produces exactly one printed trigger"
    );
    question(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}

#[test]
fn bp01_167_draws_before_selecting_a_hand_card_to_put_on_the_bottom() {
    for number in ["BP01-167", "BP01-P35"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"source","card":number},{"id":"old","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"drawn","card":"BP01-037"},{"id":"tail","card":"BP01-037"}]);
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let choose = trigger(&mut game);
        let InputDomain::Targets(domain) = &choose.domain else {
            panic!("hand");
        };
        assert_eq!(domain.members.len(), 2);
        assert!(game.internal_decision(View::P2).is_none());
        resolved(
            game.answer_internal_card(choose.actor, choose.token, Answer::Targets(vec![1]))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["old"]));
        assert_eq!(state["P1"]["deck"], json!(["tail", "drawn"]));
        assert!(game.projection(View::P2).unwrap()["objects"]["drawn"].is_null());
    }
}

#[test]
fn cp03_052_acts_and_draws_before_banishing_one_chosen_hand_card() {
    for number in ["CP03-052", "CP03-P35"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"old","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let mut game = game(&initial);
        let choose = question(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &choose.domain else {
            panic!("hand");
        };
        assert_eq!(domain.members.len(), 2);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            true
        );
        assert!(game.internal_decision(View::P2).is_none());
        let result = resolved(
            game.answer_internal_card(choose.actor, choose.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(state["P1"]["banish"], json!(["old"]));
        assert!(
            !result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Discard(_)))
        );
    }
}

#[test]
fn bp11_047_evolution_moves_only_a_chosen_witch_spell_to_ex_and_discounts_its_play() {
    for (number, take, full) in [
        ("BP11-047", true, false),
        ("EBD02-017", true, false),
        ("BP11-047", false, false),
        ("BP11-047", true, true),
    ] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP11-046"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"evolution","card":number}]);
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"spell","card":"BP01-064"},{"id":"wrong","card":"BP01-037"}]);
        if full {
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0_u8..5_u8)
                    .map(|n| json!({"id":format!("ex{n}"),"card":"BP01-037"}))
                    .collect::<Vec<_>>()
            );
        }
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "hand-evolution",
        )
        .unwrap();
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"evolution","pay":{"pp":1_u8}}),
        )
        .unwrap();
        let choose = trigger(&mut game);
        let InputDomain::Targets(domain) = &choose.domain else {
            panic!("hand");
        };
        assert_eq!(domain.members.len(), 1);
        assert_eq!(domain.cardinality.minimum, Nat(0));
        assert_eq!(domain.members[0].card.as_deref(), Some("BP01-064"));
        resolved(
            game.answer_internal_card(
                choose.actor,
                choose.token,
                Answer::Targets(if take { vec![0] } else { vec![] }),
            )
            .unwrap(),
        );
        let before = game.projection(View::Referee).unwrap();
        let moved = take && !full;
        assert_eq!(
            before["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("spell")),
            moved
        );
        assert_eq!(
            before["P1"]["hand"],
            if moved {
                json!(["wrong"])
            } else {
                json!(["spell", "wrong"])
            }
        );
        game.resume_internal_card().unwrap();
        resolved(
            game.play_internal_card("spell", PlayerId { seat: 1 })
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if moved { 7_u8 } else { 4_u8 }
        );
    }
}
