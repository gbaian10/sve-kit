#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these card scenarios."
)]
use super::*;

fn position(card: &str, count: usize) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut deck = (0..count)
        .map(|n| json!({"id":format!("up-{n}"),"card":"BP11-104","face_up":true}))
        .collect::<Vec<_>>();
    deck.push(json!({"id":"down","card":"BP11-104","face_up":false}));
    deck.push(json!({"id":"advance","card":"BP13-039","face_up":true}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!(deck);
    initial["players"]["P2"]["zones"]["evolve_deck"] =
        json!([{"id":"opponent-form","card":"BP11-104","face_up":true}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP04-117"}]);
    initial
}

fn trigger(game: &mut Game) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("trigger")
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn choose(game: &mut Game, play: CardPlay) {
    let CardPlay::Awaiting(question) = play else {
        panic!("target choice")
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("target domain")
    };
    let target = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP04-117"))
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![target]),
        )
        .unwrap(),
    );
}

#[test]
fn bp08_110_damage_counts_only_own_face_up_evolve_followers() {
    for count in [0, 2, 3] {
        let mut game = game(&position("BP08-110", count));
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let play = trigger(&mut game);
        choose(&mut game, play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], count);
    }
}

#[test]
fn bp13_110_threshold_controls_both_damage_effects() {
    for card in ["BP13-110", "BP13-P25"] {
        for count in [2, 3] {
            let mut game = game(&position(card, count));
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            let play = trigger(&mut game);
            choose(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count == 3 { 5_u8 } else { 0_u8 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count == 3 { 17_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn bp15_118_damage_draw_and_pp_at_both_thresholds() {
    for card in ["BP15-118", "PR-433"] {
        for count in [2, 3, 4, 5] {
            let mut initial = position(card, count);
            initial["players"]["P1"]["pp"] = json!({"current":5_u8, "max":10_u8});
            let mut game = game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            choose(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count >= 3 { 4_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["P1"]["hand"].as_array().unwrap().len(),
                usize::from(count >= 5)
            );
            assert_eq!(state["P1"]["pp"]["current"], 5_u8);
        }
    }
}

#[test]
fn bp11_021_evolves_only_at_three() {
    let card = "BP11-021";
    let form = "BP11-022";
    for count in [2, 3] {
        let mut initial = position(card, count);
        initial["players"]["P1"]["zones"]["evolve_deck"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"form","card":form}));
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        let play = trigger(&mut game);
        if count == 3 {
            let CardPlay::Awaiting(question) = play else {
                panic!("evolution choice")
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("evolution domain")
            };
            let handle = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(form))
                .unwrap()
                .handle;
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![handle]),
                )
                .unwrap(),
            );
        } else {
            resolved(play);
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["evolved_with"].as_str(),
            (count == 3).then_some("form")
        );
    }
}

#[test]
fn bp11_103_evolves_and_bp11_104_deals_damage_only_at_five() {
    for form in ["BP11-104", "BP11-SL20", "BP11-U07"] {
        for count in [4, 5] {
            let mut initial = position("BP11-103", count);
            initial["players"]["P1"]["zones"]["evolve_deck"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v["id"] != "down");
            initial["players"]["P1"]["zones"]["evolve_deck"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"form","card":form}));
            let mut game = game(&initial);
            assert!(matches!(
                game.play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap(),
                CardPlay::Entered(_)
            ));
            let play = trigger(&mut game);
            let CardPlay::Awaiting(question) = play else {
                panic!("evolution choice")
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("evolution domain")
            };
            let handle = domain
                .members
                .iter()
                .find(|member| member.card.as_deref() == Some(form))
                .unwrap()
                .handle;
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![handle]),
                )
                .unwrap(),
            );
            game.resume_internal_card().unwrap();
            let evolve_play = trigger(&mut game);
            choose(&mut game, evolve_play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["source"]["evolved_with"], "form");
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count == 5 { 5_u8 } else { 0_u8 }
            );
        }
    }
}

#[test]
fn bp08_054_attack_thresholds_are_independent() {
    for card in ["BP08-054", "BP08-SL12", "BP08-U04"] {
        for count in [4, 5, 7, 8] {
            let mut initial = position(card, count);
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
            let mut game = game(&initial);
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"}),
            )
            .unwrap();
            let play = trigger(&mut game);
            resolved(play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count >= 5 { 13_u8 } else { 10_u8 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count >= 8 { 17_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn bp11_022_end_phase_gives_its_controller_and_itself_two_life() {
    let mut initial = position("BP11-022", 0);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP11-022"}]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    let hp = before["objects"]["source"]["hp"].as_i64().unwrap();
    assert_eq!(before["P1"]["leader"]["life"], 10_u8);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let play = trigger(&mut game);
    resolved(play);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["source"]["hp"], hp + 2);
    assert_eq!(after["P1"]["leader"]["life"], 12_u8);
}
