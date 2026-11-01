#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate these counter scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn new_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "counter-batch",
    )
    .unwrap()
}

fn position(number: &str, zone: &str, counters: &Value) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"][zone] =
        json!([{"id":"source","card":number,"state":{"counters":counters}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":12_u8,"max_hp":12_u8,"damage":0_u8}}]);
    initial
}

fn definition(number: &str, line: u32) -> AbilityId {
    selection().current[number]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.source.line == line)
        .unwrap()
        .node
        .id
}

fn finish(game: &mut Game, play: CardPlay) -> CardPlay {
    finish_result(game, play).unwrap()
}

fn finish_result(game: &mut Game, mut play: CardPlay) -> Result<CardPlay, EngineFailure> {
    while let CardPlay::Awaiting(question) = play {
        let answer = counter_answer(&question.domain);
        play = game.answer_internal_card(question.actor, question.token, answer)?;
    }
    Ok(play)
}

fn counter_answer(domain: &InputDomain) -> Answer {
    match domain {
        InputDomain::Targets(domain) => Answer::Targets(vec![
            domain
                .members
                .iter()
                .find(|m| m.card.is_some())
                .unwrap()
                .handle,
        ]),
        InputDomain::WardEntry { .. } => Answer::WardEntry(false),
        _ => panic!("unexpected counter question"),
    }
}

fn play(game: &mut Game, holder: &str) {
    let step = game.play_internal_card(holder, PLAYER).unwrap();
    match finish(game, step) {
        CardPlay::Entered(_) => {}
        CardPlay::Resolved(_) => {
            game.resume_internal_card().unwrap();
        }
        _ => panic!("successful printed play"),
    }
}

fn trigger(game: &mut Game) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed Fanfare")
    };
    assert_eq!(
        options.len(),
        1,
        "exactly one printed counter trigger is pending"
    );
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    resolved(finish(game, step));
    game.resume_internal_card().unwrap();
}

#[test]
fn flower_princess_and_printing_always_create_a_fairy_but_need_real_combo_three_for_a_counter() {
    for number in ["BP03-012", "PR-053"] {
        for (prior, damage, combo) in [(1_usize, 3_u8, false), (2, 6, true)] {
            let mut initial = position(number, "hand", &json!({}));
            for n in 0..prior {
                initial["players"]["P1"]["zones"]["hand"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"id":format!("spell-{n}"),"card":"BP01-168"}));
            }
            let mut game = new_game(&initial);
            for n in 0..prior {
                play(&mut game, &format!("spell-{n}"));
            }
            play(&mut game, "source");
            trigger(&mut game);
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(before["P1"]["ex"].as_array().unwrap().len(), 1);
            assert_eq!(before["objects"]["enemy"]["damage"], damage);
            if combo {
                assert_eq!(
                    before["objects"]["source"]["counters"]["童話カウンター"],
                    1_u8
                );
                let step = game
                    .activate_internal_card("source", definition(number, 2), PLAYER)
                    .unwrap();
                resolved(finish(&mut game, step));
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(after["objects"]["enemy"]["damage"], 9_u8);
                assert_eq!(
                    after["objects"]["source"]["counters"]["童話カウンター"],
                    0_u8
                );
                assert_eq!(after["objects"]["source"]["acted"], true);
            } else {
                let play = game
                    .activate_internal_card("source", definition(number, 2), PLAYER)
                    .unwrap();
                assert!(matches!(
                    finish_result(&mut game, play).unwrap_err(),
                    EngineFailure::IllegalPlay(_)
                ));
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
                assert_eq!(after["objects"]["source"], before["objects"]["source"]);
            }
        }
    }
}

#[test]
fn tin_soldier_and_printing_receive_the_counter_from_ex_but_not_from_hand() {
    for number in ["BP03-101", "PR-125"] {
        for zone in ["hand", "ex"] {
            let mut game = new_game(&position(number, zone, &json!({})));
            play(&mut game, "source");
            trigger(&mut game);
            if zone == "ex" {
                let step = game
                    .activate_internal_card("source", definition(number, 2), PLAYER)
                    .unwrap();
                resolved(finish(&mut game, step));
                let after = game.projection(View::Referee).unwrap();
                assert_eq!(after["objects"]["enemy"]["damage"], 2_u8);
                assert_eq!(
                    after["objects"]["source"]["counters"]["童話カウンター"],
                    0_u8
                );
            } else {
                let play = game
                    .activate_internal_card("source", definition(number, 2), PLAYER)
                    .unwrap();
                assert!(matches!(
                    finish_result(&mut game, play).unwrap_err(),
                    EngineFailure::IllegalPlay(_)
                ));
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                    0_u8
                );
            }
        }
    }
}

#[test]
fn kagemitsu_and_printing_pay_one_or_five_war_counters_for_their_printed_effects() {
    for number in ["BP15-022", "BP15-SL08"] {
        for (count, line, damage, life, storm) in
            [(1_u8, 1_u32, 1_u8, 20_u8, false), (5, 2, 5, 15, true)]
        {
            let mut game = new_game(&position(number, "field", &json!({"戦意カウンター":count})));
            let step = game
                .activate_internal_card("source", definition(number, line), PLAYER)
                .unwrap();
            resolved(finish(&mut game, step));
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["enemy"]["damage"], damage);
            assert_eq!(after["P2"]["leader"]["life"], life);
            assert_eq!(
                after["objects"]["source"]["counters"]["戦意カウンター"],
                0_u8
            );
            assert_eq!(
                after["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                storm
            );
        }
        let mut blocked_game =
            new_game(&position(number, "field", &json!({"戦意カウンター":4_u8})));
        assert!(matches!(
            blocked_game
                .activate_internal_card("source", definition(number, 2), PLAYER)
                .unwrap_err(),
            EngineFailure::IllegalPlay(_)
        ));
        assert_eq!(
            blocked_game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["戦意カウンター"],
            4_u8
        );
    }
}

#[test]
fn hot_spring_places_ten_counters_and_its_end_damage_requires_the_last_one_to_be_removed() {
    for number in ["BP14-059", "BP14-P13"] {
        let mut entered_game = new_game(&position(number, "hand", &json!({})));
        play(&mut entered_game, "source");
        trigger(&mut entered_game);
        assert_eq!(
            entered_game.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["神湯カウンター"],
            10_u8
        );
        for count in [1_u8, 2] {
            let mut game = new_game(&position(number, "field", &json!({"神湯カウンター":count})));
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            trigger(&mut game);
            let after = game.projection(View::Referee).unwrap();
            if count == 1 {
                assert_eq!(after["P1"]["cemetery"], json!(["source"]));
                assert_eq!(after["objects"]["enemy"]["damage"], 5_u8);
            } else {
                assert_eq!(after["P1"]["field"], json!(["source"]));
                assert_eq!(
                    after["objects"]["source"]["counters"]["神湯カウンター"],
                    1_u8
                );
                assert_eq!(after["objects"]["enemy"]["damage"], 0_u8);
            }
            assert_eq!(after["P2"]["leader"]["life"], 20_u8);
        }
    }
}

#[test]
fn cursed_tool_increments_before_testing_each_threshold_and_retires_only_at_three() {
    for (before, life, hand, retired) in [
        (0_u8, 19_u8, 2_usize, false),
        (1, 20, 1, false),
        (2, 17, 1, true),
    ] {
        let mut initial = position("BP18-T09", "field", &json!({"呪いカウンター":before}));
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"hand1","card":"BP01-037"},{"id":"hand2","card":"BP01-037"}
        ]);
        let mut game = new_game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], life);
        assert_eq!(state["P1"]["hand_count"], hand);
        assert_eq!(
            !state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            retired
        );
    }
}

#[test]
fn chamber_and_its_promo_share_real_fanfare_draw_and_counter_payment() {
    for number in ["BP19-091", "PR-535"] {
        let mut initial = position(number, "hand", &json!({}));
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        let mut game = new_game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let fanfare_state = game.projection(View::Referee).unwrap();
        assert_eq!(fanfare_state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(
            fanfare_state["objects"]["source"]["counters"]["加虐カウンター"],
            2_u8
        );
        let play = game
            .activate_internal_card("source", definition(number, 2), PLAYER)
            .unwrap();
        resolved(finish(&mut game, play));
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 19_u8);
        assert_eq!(
            after["objects"]["source"]["counters"]["加虐カウンター"],
            1_u8
        );
        assert_eq!(after["objects"]["source"]["acted"], true);
    }
}
