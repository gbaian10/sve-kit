#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
const PLAYER: PlayerId = PlayerId { seat: 1 };
fn position(number: &str, count: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"mine0","card":"BP01-080"},{"id":"mine1","card":"BP01-080"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!(
        (0..count)
            .map(|i| json!({"id":format!("top{i}"),"card":"BP01-080"}))
            .collect::<Vec<_>>()
    );
    initial
}
fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn finish(game: &mut Game, mut step: CardPlay, mode: usize, discard: usize) {
    while let CardPlay::Awaiting(question) = step {
        let answer = match &question.domain {
            InputDomain::Modes { modes, .. } => {
                assert_eq!(modes.len(), 2, "both printed choices remain available");
                Answer::Modes(vec![modes[mode]])
            }
            InputDomain::Targets(domain) => {
                assert_eq!(
                    domain.members.len(),
                    2,
                    "the original hand card and the newly drawn card may both be discarded"
                );
                Answer::Targets(vec![domain.members[discard].handle])
            }
            _ => panic!("unexpected printed decision: {:?}", question.domain),
        };
        step = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        !matches!(step, CardPlay::Cancelled),
        "the printed action has all required resources"
    );
    checks(game);
}
fn assert_mill(game: &Game, count: usize, milled: usize) {
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["cemetery"],
        json!((0..milled).map(|i| format!("top{i}")).collect::<Vec<_>>()),
        "only the opponent top cards reach their owner's cemetery in order"
    );
    assert_eq!(
        state["P2"]["deck"],
        json!(
            (milled..count)
                .map(|i| format!("top{i}"))
                .collect::<Vec<_>>()
        ),
        "the opponent's remaining deck keeps its order"
    );
    let public = game.projection(View::P1).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    for i in 0..count {
        assert_eq!(
            known.iter().any(|card| card["id"] == format!("top{i}")),
            i < milled,
            "milled cards become public while the unselected opponent deck stays private"
        );
    }
}
#[test]
fn vivid_theft_mills_up_to_three_opponent_cards_and_is_available_in_an_opponent_turn_quick_window()
{
    for number in ["BP06-028", "PR-526"] {
        for count in 0..=4_usize {
            let mut game = game(&position(number, count));
            let step = game.play_internal_card("source", PLAYER).unwrap();
            finish(&mut game, step, 0, 0);
            assert_mill(&game, count, count.min(3));
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["deck"],
                json!(["mine0", "mine1"]),
                "the spell does not mill its controller's deck"
            );
            assert_eq!(
                state["P1"]["cemetery"],
                json!(["source"]),
                "the resolved spell itself enters its owner's cemetery"
            );
            assert_eq!(
                state["P1"]["pp"]["current"], 9_u8,
                "the printed one PP is paid"
            );
        }
        let mut quick_initial = position(number, 4);
        quick_initial["turn"]["active"] = json!("P2");
        let mut quick_game = game(&quick_initial);
        quick_game
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        let question = quick_game.internal_quick_choice().unwrap().unwrap();
        assert_eq!(
            question.actor, PLAYER,
            "the nonactive player receives the Quick window"
        );
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("the opponent end phase opens a Quick window")
        };
        assert_eq!(
            options.len(),
            1,
            "the printed Quick spell is available from hand"
        );
        assert_eq!(
            options[0].card, number,
            "the option is this printing's complete spell"
        );
        let step = quick_game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Quick(Some(options[0].handle)),
            )
            .unwrap();
        finish(&mut quick_game, step, 0, 0);
        assert_mill(&quick_game, 4, 3);
        assert_eq!(
            quick_game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9_u8,
            "Quick still pays the printed spell cost"
        );
    }
}
#[test]
fn thief_girl_can_mill_the_opponent_or_draw_then_choose_either_old_or_new_hand_card_to_discard() {
    for number in ["BP06-031", "PR-131"] {
        for mode in [0_usize, 1] {
            for discard in [0_usize, 1] {
                for count in [0_usize, 2] {
                    let mut initial = position(number, count);
                    initial["players"]["P1"]["zones"]["hand"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"id":"old","card":"BP01-080"}));
                    let mut game = game(&initial);
                    let played = game.play_internal_card("source", PLAYER).unwrap();
                    finish(&mut game, played, mode, discard);
                    let question = game.internal_pending_choice().unwrap().unwrap();
                    let InputDomain::Pending(options) = &question.domain else {
                        panic!("the printed fanfare is pending")
                    };
                    assert_eq!(options.len(), 1, "exactly this fanfare is pending");
                    let step = game
                        .answer_internal_card(
                            question.actor,
                            question.token,
                            Answer::Pending(options[0].handle),
                        )
                        .unwrap();
                    finish(&mut game, step, mode, discard);
                    assert_mill(&game, count, if mode == 0 { count.min(1) } else { 0 });
                    let state = game.projection(View::Referee).unwrap();
                    assert_eq!(
                        state["P1"]["field"],
                        json!(["source"]),
                        "the follower remains in play after either choice"
                    );
                    assert_eq!(
                        state["P1"]["pp"]["current"], 8_u8,
                        "either choice pays only the follower's printed cost"
                    );
                    assert_eq!(
                        state["P1"]["deck"],
                        if mode == 0 {
                            json!(["mine0", "mine1"])
                        } else {
                            json!(["mine1"])
                        },
                        "only the draw alternative changes the controller's deck"
                    );
                    assert_eq!(
                        state["P1"]["hand"],
                        if mode == 0 || discard == 1 {
                            json!(["old"])
                        } else {
                            json!(["mine0"])
                        },
                        "drawing precedes the mandatory discard and either hand card may be selected"
                    );
                    assert_eq!(
                        state["P1"]["cemetery"],
                        if mode == 0 {
                            json!([])
                        } else if discard == 0 {
                            json!(["old"])
                        } else {
                            json!(["mine0"])
                        },
                        "only the chosen hand card is discarded"
                    );
                }
            }
        }
    }
}
