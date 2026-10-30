#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate the real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::{Answer, CardPlay, InputDomain, ResumeStage};
use sve_engine::ir::PlayerId;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str, face_up: usize, face_down: usize) -> Value {
    let evolve_card = match card {
        "BP08-054" => "BP08-054",
        "BP08-110" => "BP08-111",
        "BP11-103" => "BP11-104",
        "BP15-004" => "BP15-005",
        _ => "BP11-022",
    };
    let evolves =
        (0..face_up)
            .map(|index| json!({"id":format!("up-{index}"),"card":evolve_card,"face_up":true}))
            .chain((0..face_down).map(
                |index| json!({"id":format!("down-{index}"),"card":evolve_card,"face_up":false}),
            ))
            .collect::<Vec<_>>();
    json!({
        "history":"explicit",
        "turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3,"P2":3}},
        "players":{
            "P1":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":10,"max":10},"ep":0,"sep":0,
                "zones":{"hand":[{"id":"source","card":card}],"evolve_deck":evolves,"field":[]}},
            "P2":{"leader":{"class":"ニュートラル","life":20},"pp":{"current":10,"max":10},"ep":0,"sep":0,
                "zones":{"field":[{"id":"enemy","card":"BP01-037","state":{"acted":true}}]}}
        }
    })
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match question.domain {
            InputDomain::Targets(domain) => Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            ),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Quick { .. } => Answer::Quick(None),
            InputDomain::OptionalCost
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::Search(_)
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::Modes { .. } => panic!("unexpected card decision"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn play_and_resolve_fanfare(game: &mut Game, card: &str) {
    let play = game.play_internal_card("source", actor()).unwrap();
    let CardPlay::Entered(entry) = finish(game, play) else {
        panic!("the real card enters the field");
    };
    if matches!(entry.resume, ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("the card has a pending Fanfare");
    };
    assert!(
        options.iter().any(|option| option.card == card),
        "the printed Fanfare is pending"
    );
    let pending = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    assert!(
        matches!(finish(game, pending), CardPlay::Resolved(_)),
        "the printed Fanfare resolves"
    );
    game.resume_internal_card().unwrap();
    loop {
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(trigger_question) = game.internal_pending_choice().unwrap() else {
            break;
        };
        let InputDomain::Pending(trigger_options) = trigger_question.domain else {
            panic!("an evolved card trigger is pending");
        };
        let trigger_play = game
            .answer_internal_card(
                trigger_question.actor,
                trigger_question.token,
                Answer::Pending(trigger_options[0].handle),
            )
            .unwrap();
        assert!(
            matches!(finish(game, trigger_play), CardPlay::Resolved(_)),
            "the evolved card trigger resolves"
        );
        game.resume_internal_card().unwrap();
    }
}

#[test]
fn bp11_021_evolves_at_three_face_up_evolve_followers_but_not_two() {
    for (face_up, evolves) in [(2, false), (3, true)] {
        let mut game = game(&position("BP11-021", face_up, 1));
        play_and_resolve_fanfare(&mut game, "BP11-021");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["evolved"], evolves);
        assert_eq!(
            state["P1"]["evolve_deck"].as_array().unwrap().len(),
            face_up + 1 - usize::from(evolves)
        );
    }
}

#[test]
fn bp11_022_has_assail_and_heals_itself_and_its_leader_at_end_phase() {
    let mut initial = position("BP11-021", 2, 1);
    initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP11-021","state":{"entered_this_turn":false,"damage":2_i32}}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    let mut attack_game = game(&initial);
    assert_eq!(
        attack_game
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"down-0","pay":{"pp":2_i32}})
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    assert_eq!(
        attack_game
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    let quick = attack_game.internal_quick_choice().unwrap().unwrap();
    assert!(matches!(
        attack_game
            .answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap(),
        CardPlay::Continued(_)
    ));

    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"down-0","pay":{"pp":2_i32}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("the evolved follower's End-phase ability is pending");
    };
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    assert!(matches!(finish(&mut game, play), CardPlay::Resolved(_)));
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 12_i32);
    assert_eq!(state["objects"]["source"]["damage"], 2_i32);
    assert_eq!(state["objects"]["source"]["hp"], 5_i32);
}

#[test]
fn bp11_103_fanfare_evolves_without_a_count_threshold() {
    let mut initial = position("BP11-103", 0, 2);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"down-0","card":"BP11-104","face_up":false},
            {"id":"down-1","card":"BP11-104","face_up":false}]);
    let mut game = game(&initial);
    play_and_resolve_fanfare(&mut game, "BP11-103");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["evolved"], true);
    assert_eq!(state["objects"]["source"]["evolved_with"], "down-0");
    assert_eq!(state["P1"]["evolve_deck"].as_array().unwrap().len(), 1);
}

#[test]
fn bp11_104_evolve_time_damages_only_at_five_face_up() {
    for (face_up, expected_damage) in [(4, 0_i32), (5, 5_i32)] {
        let mut game = game(&position("BP11-103", face_up, 1));
        play_and_resolve_fanfare(&mut game, "BP11-103");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["field"].as_array().unwrap().is_empty(),
            expected_damage == 5_i32
        );
        assert_eq!(state["objects"]["source"]["evolved"], true);
    }
}

#[test]
fn bp13_110_damages_target_and_leader_at_three_but_not_two() {
    for (face_up, damaged) in [(2, false), (3, true)] {
        let mut game = game(&position("BP13-110", face_up, 1));
        play_and_resolve_fanfare(&mut game, "BP13-110");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["leader"]["life"],
            if damaged { 17_i32 } else { 20_i32 }
        );
        assert_eq!(state["P2"]["field"].as_array().unwrap().is_empty(), damaged);
    }
}

#[test]
fn bp15_004_does_not_self_evolve_below_three_face_up() {
    let mut game = game(&position("BP15-004", 2, 1));
    play_and_resolve_fanfare(&mut game, "BP15-004");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["leader"]["life"], 20_i32);
    assert_eq!(state["objects"]["source"]["evolved"], false);
}

#[test]
fn bp15_004_triggers_when_another_follower_evolves() {
    let mut initial = position("BP15-004", 0, 0);
    initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP15-004","state":{"entered_this_turn":false}},
        {"id":"ally","card":"BP11-021","state":{"entered_this_turn":false}}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"BP11-022","face_up":false}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"ally","evolve_card":"form","pay":{"pp":2_i32}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = question.domain else {
        panic!("BP15-004 waits on the ally's evolution");
    };
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    assert!(matches!(finish(&mut game, play), CardPlay::Resolved(_)));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 19_i32);
    assert_eq!(state["objects"]["enemy"]["damage"], 2_i32);
    assert_eq!(state["P1"]["leader"]["life"], 11_i32);
}

#[test]
fn bp15_118_changes_damage_and_draws_and_recovers_pp_at_five() {
    let mut pps = Vec::new();
    for (face_up, damage, hand) in [(2, 2_i32, 0_usize), (5, 4_i32, 1_usize)] {
        let mut initial = position("BP15-118", face_up, 0);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
        initial["players"]["P1"]["pp"]["current"] = json!(9_i32);
        let mut game = game(&initial);
        let play = game.play_internal_card("source", actor()).unwrap();
        assert!(matches!(finish(&mut game, play), CardPlay::Resolved(_)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), hand);
        pps.push(state["P1"]["pp"]["current"].as_u64().unwrap());
        assert_eq!(state["objects"]["enemy"]["damage"], damage);
    }
    assert_eq!(pps[1], pps[0] + 1);
}

#[test]
fn bp08_054_has_storm_and_applies_each_attack_threshold() {
    for (face_up, life) in [(4, 10_i32), (5, 13_i32), (8, 13_i32)] {
        let mut initial = position("BP08-054", face_up, 0);
        initial["players"]["P1"]["leader"]["life"] = json!(10_i32);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP08-054","state":{"entered_this_turn":false}}]);
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap();
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = pending.domain else {
            panic!("the printed attack ability is pending");
        };
        let play = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        assert!(matches!(finish(&mut game, play), CardPlay::Resolved(_)));
        game.resume_internal_card().unwrap();
        let question = game.internal_quick_choice().unwrap().unwrap();
        assert!(matches!(
            game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                .unwrap(),
            CardPlay::Continued(_)
        ));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], life);
        assert_eq!(
            state["P2"]["leader"]["life"],
            if face_up == 8 { 10_i32 } else { 13_i32 }
        );
    }
}

#[test]
fn bp08_110_deals_damage_equal_to_the_face_up_count() {
    for (face_up, expected_damage) in [(0, 0_i32), (2, 2_i32)] {
        let mut game = game(&position("BP08-110", face_up, 0));
        play_and_resolve_fanfare(&mut game, "BP08-110");
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], expected_damage);
    }
}
