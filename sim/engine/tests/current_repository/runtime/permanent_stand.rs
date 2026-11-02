#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed stand restriction scenarios."
)]
use super::*;

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "permanent-stand",
    )
    .unwrap()
}

fn initial(card: &str, hand: bool) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["zones"]["field"] = json!([]);
    value["players"]["P1"]["zones"]["hand"] = json!([]);
    value["players"]["P2"]["zones"]["field"] = json!([]);
    value["players"]["P1"]["zones"][if hand { "hand" } else { "field" }] =
        json!([{"id":"source","card":card,"state":{"acted":!hand}}]);
    for player in ["P1", "P2"] {
        value["players"][player]["zones"]["deck"] = json!(
            (0..6_u8)
                .map(|i| json!({"id":format!("{player}-draw{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
    }
    value
}

fn finish(game: &mut Game, mut play: CardPlay, accept_cost: bool) {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::OptionalCost => {
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                    true,
                    "the rule-owned Start stand stays blocked before the Main payment"
                );
                Answer::OptionalCost(accept_cost)
            }
            InputDomain::Quick { .. } => Answer::Quick(None),
            _ => panic!("only Main payment or phase-end quick window"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(
        matches!(
            play,
            CardPlay::Resolved(_)
                | CardPlay::Entered(_)
                | CardPlay::Cancelled
                | CardPlay::Continued(_)
        ),
        "the printed occurrence either resolves or is declined"
    );
}

fn settle(game: &mut Game, accept_cost: bool) {
    loop {
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("printed pending trigger");
        };
        let play = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, play, accept_cost);
    }
}

fn end(game: &mut Game, accept_cost: bool) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(game, accept_cost);
    while let Some(question) = game.internal_quick_choice().unwrap() {
        let play = game
            .answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish(game, play, accept_cost);
        settle(game, accept_cost);
    }
}

#[test]
fn ecp01_053_permanent_restriction_preserves_the_optional_main_payment() {
    for card in ["ECP01-053", "ECP01-P29"] {
        for accept_cost in [false, true] {
            let mut game = game(&initial(card, false));
            end(&mut game, accept_cost);
            end(&mut game, accept_cost);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["acted"], !accept_cost);
            assert_eq!(
                state["P1"]["pp"]["current"],
                if accept_cost { 9_u8 } else { 10 }
            );
            if !accept_cost {
                end(&mut game, false);
                end(&mut game, false);
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                    true
                );
            }
        }
    }
}

#[test]
fn cp02_025_fanfare_acts_a_small_idol_field_then_main_can_stand_it() {
    for card in ["CP02-025", "CP02-P19", "PR-144"] {
        let mut game = game(&initial(card, true));
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play, true);
        settle(&mut game, true);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
            true
        );
        end(&mut game, true);
        end(&mut game, true);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["acted"], false);
        assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    }
    let mut position = initial("CP02-025", true);
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"other1","card":"CP02-025"},{"id":"other2","card":"CP02-025"}]);
    let mut game = game(&position);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play, false);
    settle(&mut game, false);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
        false,
        "three Deremas followers exceed the printed fanfare threshold"
    );
}
