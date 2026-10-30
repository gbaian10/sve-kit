#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate printed face-up evolution scenarios."
)]
use super::*;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, count: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"}
    ]);
    let mut cards = (0..count)
        .map(|i| {
            json!({
                "id":format!("used-{i}"),"card":"BP02-119","face_up":true
            })
        })
        .collect::<Vec<_>>();
    cards.extend([
        json!({"id":"down","card":"BP02-119","face_up":false}),
        json!({"id":"unset","card":"BP02-119"}),
        json!({"id":"ordinary-follower","card":"BP01-037","face_up":true}),
        json!({"id":"other-kind","card":"BP01-016","face_up":true}),
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!(cards);
    initial["players"]["P2"]["zones"]["evolve_deck"] = json!([
        {"id":"opponent-used","card":"BP02-119","face_up":true}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "faceup-evolve",
    )
    .unwrap()
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(domain.members.iter().take(1).map(|m| m.handle).collect())
        } else if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("unexpected printed choice");
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    assert!(!matches!(play, CardPlay::Cancelled), "the printed play has its required resources and targets");
    checks(game);
}

fn play(game: &mut Game) {
    let play = game.play_internal_card("source", actor()).unwrap();
    finish(game, play);
}

fn trigger(game: &mut Game, card: &str) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    let option = options.iter().find(|option| option.card == card).unwrap();
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap();
    finish(game, play);
}

fn form(initial: &mut Value, card: &str) {
    initial["players"]["P1"]["zones"]["evolve_deck"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"form","card":card}));
}

#[test]
fn bp11_021_evolves_only_at_three_own_face_up_evolve_followers() {
    for count in [0_usize, 2, 3] {
        let mut initial = position("BP11-021", count);
        form(&mut initial, "BP11-022");
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game, "BP11-021");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if count >= 3 { 4_u8 } else { 3 }
        );
        assert_eq!(
            state["P1"]["evolve_deck"]
                .as_array()
                .unwrap()
                .contains(&json!("form")),
            count < 3
        );
        assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    }
}

#[test]
fn bp11_022_printings_gain_life_only_at_their_own_end_phase() {
    for card in ["BP11-022", "PR-315"] {
        for active in ["P1", "P2"] {
            let mut initial = position(card, 0);
            initial["turn"]["active"] = json!(active);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            let mut game = game(&initial);
            assert_eq!(
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap()
                    .outcome,
                "resolved"
            );
            checks(&mut game);
            if active == "P1" {
                trigger(&mut game, card);
            } else {
                assert!(game.internal_pending_choice().unwrap().is_none());
            }
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["hp"],
                if active == "P1" { 7_u8 } else { 5 }
            );
            assert_eq!(
                state["P1"]["leader"]["life"],
                if active == "P1" { 12_u8 } else { 10 }
            );
            assert!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("指定攻撃"))
            );
        }
    }
}

#[test]
fn bp11_103_printings_evolve_even_without_used_evolve_cards() {
    for card in ["BP11-103", "BP11-SL19"] {
        for available in [false, true] {
            let mut initial = position(card, 0);
            if available { form(&mut initial, "BP11-104"); }
            let mut game = game(&initial);
            play(&mut game);
            trigger(&mut game, card);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["power"], if available {3_u8} else {2});
            if available { trigger(&mut game, "BP11-104"); }
            else { assert!(game.internal_pending_choice().unwrap().is_none()); }
            assert_eq!(game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"], 0_u8);
        }
    }
}

#[test]
fn bp11_104_printings_damage_only_at_five_after_actual_effect_evolution() {
    for card in ["BP11-104", "BP11-SL20", "BP11-U07"] {
        for count in [4_usize, 5] {
            let mut initial = position("BP11-103", count);
            form(&mut initial, card);
            let mut game = game(&initial);
            play(&mut game);
            trigger(&mut game, "BP11-103");
            trigger(&mut game, card);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if count == 5 { 5_u8 } else { 0 }
            );
        }
    }
}

#[test]
fn bp13_110_printings_gate_both_follower_and_its_leader_damage() {
    for card in ["BP13-110", "BP13-P25"] {
        for count in [2_usize, 3] {
            let mut game = game(&position(card, count));
            play(&mut game);
            trigger(&mut game, card);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count == 3 { 5_u8 } else { 0 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count == 3 { 17_u8 } else { 20 }
            );
        }
    }
}

#[test]
fn bp15_118_printings_separate_damage_draw_and_unconditional_pp_recovery() {
    for card in ["BP15-118", "PR-433"] {
        for count in [0_usize, 2, 3, 4, 5] {
            let mut game = game(&position(card, count));
            play(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count >= 3 { 4_u8 } else { 2 }
            );
            assert_eq!(
                state["P1"]["hand"],
                if count >= 5 {
                    json!(["top"])
                } else {
                    json!([])
                }
            );
            assert_eq!(state["P1"]["pp"]["current"], 10_u8);
        }
    }
}

#[test]
fn bp08_054_printings_apply_independent_attack_thresholds() {
    for card in ["BP08-054", "BP08-SL12", "BP08-U04"] {
        for count in [4_usize, 5, 7, 8] {
            let mut initial = position(card, count);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            let mut game = game(&initial);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            trigger(&mut game, card);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count >= 5 { 13_u8 } else { 10 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count >= 8 { 17_u8 } else { 20 }
            );
        }
    }
}

#[test]
fn bp08_110_damage_counts_only_own_face_up_evolve_followers() {
    for count in [0_usize, 1, 4] {
        let mut initial = position("BP08-110", count);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"wrong-zone","card":"BP02-119","face_up":true}
        ]);
        let mut game = game(&initial);
        play(&mut game);
        trigger(&mut game, "BP08-110");
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            u64::try_from(count).unwrap()
        );
    }
}

#[test]
fn bp08_110_count_is_relative_to_the_second_seat_controller() {
    let mut initial = position("BP08-110", 3);
    let first = initial["players"]["P1"].clone();
    initial["players"]["P1"] = initial["players"]["P2"].clone();
    initial["players"]["P2"] = first;
    initial["turn"]["active"] = json!("P2");
    let mut game = game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 2 })
        .unwrap();
    finish(&mut game, play);
    trigger(&mut game, "BP08-110");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        3_u8
    );
}

#[test]
fn unadmitted_advance_cards_remain_rejected_as_evolution_resources() {
    let mut initial = position("BP08-110", 0);
    form(&mut initial, "BP10-004");
    let result = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "advance-resource-boundary",
    );
    assert!(failure(result).contains("evolution resource must have printed Evolve type"));
}
