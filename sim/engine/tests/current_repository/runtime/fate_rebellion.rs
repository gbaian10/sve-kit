use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::InstructionOutcome;

fn position(number: &str, actor: &str) -> (Value, Value) {
    let mut initial = draw_position();
    initial["turn"]["active"] = json!(actor);
    let mut shuffles = Vec::new();
    for player in [actor, if actor == "P1" { "P2" } else { "P1" }] {
        for (zone, prefix, count) in [("hand", "h", 2_u8), ("cemetery", "g", 2), ("deck", "d", 4)] {
            initial["players"][player]["zones"][zone] =
                json!((0..count)
                .map(|index| json!({"id":format!("{player}-{prefix}{index}"),"card":"BP01-171"}))
                .collect::<Vec<_>>());
        }
        shuffles.push(json!({"player":player,"result":[
            format!("{player}-h0"),format!("{player}-h1"),format!("{player}-g0"),format!("{player}-g1"),
            format!("{player}-d0"),format!("{player}-d1"),format!("{player}-d2"),format!("{player}-d3")
        ]}));
    }
    initial["players"][actor]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"source","card":number}));
    (initial, json!({"shuffles":shuffles}))
}

fn board(initial: &Value, random: &Value) -> Game {
    Game::from_selected_effects(
        selection().clone(),
        initial,
        &Value::Null,
        random,
        "fate-rebellion",
    )
    .unwrap()
}

#[test]
fn fate_rebellion_all_printings_banish_before_the_joint_reset_and_remain_banished() {
    for number in ["BP12-044", "BP12-SL13"] {
        for actor in ["P1", "P2"] {
            let (initial, random) = position(number, actor);
            let mut game = board(&initial, &random);
            let result = resolved(
                game.play_internal_card(
                    "source",
                    PlayerId {
                        seat: seat_number(actor),
                    },
                )
                .unwrap(),
            );
            let trace = result
                .progress
                .facts
                .iter()
                .filter_map(|fact| {
                    if let Fact::Movement(moved) = fact {
                        Some(if moved.method == MoveMethod::Banish {
                            "banish"
                        } else {
                            "return"
                        })
                    } else if matches!(fact, Fact::Shuffled { .. }) {
                        Some("shuffle")
                    } else if matches!(fact, Fact::Draw(_)) {
                        Some("draw")
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let mut expected = vec!["banish"];
            expected.extend(["return"; 8]);
            expected.extend(["shuffle"; 2]);
            expected.extend(["draw"; 10]);
            assert_eq!(trace, expected);
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view[actor]["banish"], json!(["source"]));
            assert_eq!(view["objects"]["source"]["generation"], 2_u8);
            assert_eq!(view[actor]["pp"]["current"], 3_u8);
            for player in ["P1", "P2"] {
                assert_eq!(view[player]["cemetery"], json!([]));
                assert_eq!(
                    view[player]["hand"],
                    json!([
                        format!("{player}-h0"),
                        format!("{player}-h1"),
                        format!("{player}-g0"),
                        format!("{player}-g1"),
                        format!("{player}-d0")
                    ])
                );
                assert_eq!(view[player]["deck"].as_array().unwrap().len(), 3);
            }
            for (viewer, opponent) in [(View::P1, "P2"), (View::P2, "P1")] {
                let private = game.projection(viewer).unwrap();
                assert!(private["objects"].get(format!("{opponent}-h0")).is_none());
                assert!(private["objects"].get(format!("{opponent}-d0")).is_none());
            }
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()[actor]["cemetery"],
                json!([])
            );
        }
    }
}

fn end_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if let Some(question) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
        finish_checks(game);
    }
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
}

#[test]
fn fate_rebellion_discounts_only_the_controllers_next_card_and_expires_this_turn() {
    for expire in [false, true] {
        let (initial, random) = position("BP12-044", "P1");
        let mut game = board(&initial, &random);
        resolved(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        finish_checks(&mut game);
        if expire {
            end_turn(&mut game);
            let before = game.projection(View::Referee).unwrap()["P2"]["pp"]["current"]
                .as_u64()
                .unwrap();
            game.play_internal_card("P2-h0", PlayerId { seat: 2 })
                .unwrap();
            finish_checks(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["pp"]["current"],
                before - 1
            );
            end_turn(&mut game);
        }
        let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
            .as_u64()
            .unwrap();
        game.play_internal_card("P1-h0", PlayerId { seat: 1 })
            .unwrap();
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            before - u64::from(expire)
        );
        game.play_internal_card("P1-h1", PlayerId { seat: 1 })
            .unwrap();
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            before - u64::from(expire) - 1
        );
    }
}

#[test]
fn fate_rebellion_short_decks_finish_both_draws_before_the_loss_check() {
    for opponent_count in [2_u8, 5] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP12-044"}]);
        for (player, count) in [("P1", 2_u8), ("P2", opponent_count)] {
            initial["players"][player]["zones"]["deck"] = json!(
                (0..count)
                    .map(|index| json!({"id":format!("{player}-d{index}"),"card":"BP01-171"}))
                    .collect::<Vec<_>>()
            );
        }
        let mut game = game(&initial);
        let result = resolved(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        let actual = u64::from(2 + opponent_count);
        assert!(result.progress.receipts.values().any(|outcome| outcome
            == &InstructionOutcome::Cards(CardQuantityOutcome {
                requested: Int(10),
                actual: Nat(actual)
            })));
        let before = game.projection(View::Referee).unwrap();
        assert_ne!(before["game"]["ended"], true);
        assert_eq!(before["P1"]["hand"].as_array().unwrap().len(), 2);
        assert_eq!(
            before["P2"]["hand"].as_array().unwrap().len(),
            usize::from(opponent_count)
        );
        finish_checks(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["game"]["ended"], true);
        assert_eq!(
            after["game"]["winner"],
            if opponent_count == 2 {
                Value::Null
            } else {
                json!("P2")
            }
        );
        assert_eq!(after["P1"]["banish"], json!(["source"]));
        assert_eq!(after["P1"]["cemetery"], json!([]));
    }
}

#[test]
fn fate_rebellion_failure_after_self_banish_restores_the_entire_play_for_retry() {
    let (initial, mut random) = position("BP12-044", "P1");
    random["shuffles"][1]["result"] = json!(["missing"]);
    let mut game = board(&initial, &random);
    let before = format!("{game:?}");
    for _ in 0_u8..2 {
        let error = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("scripted shuffle is not a permutation")
        );
        assert_eq!(format!("{game:?}"), before);
    }
}
