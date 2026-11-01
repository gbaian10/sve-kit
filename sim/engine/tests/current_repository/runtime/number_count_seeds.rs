#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected questions invalidate these numeric scenarios."
)]
use super::*;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn new_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "number-count",
    )
    .unwrap()
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = match &question.domain {
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) => {
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            _ => panic!("unexpected numeric question"),
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(
        options.len(),
        1,
        "exactly one printed numeric trigger is pending"
    );
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    resolved(finish(game, play));
}

#[test]
fn abby_reads_her_current_attack_after_the_printed_increase() {
    for (power, increased, life) in [(3_u8, 4_u8, 20_u8), (4, 5, 18)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"BP03-008","state":{"power":power}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"power":0_u8,"hp":30_u8,"max_hp":30_u8}}]);
        let mut game = new_game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], increased);
        assert_eq!(state["P2"]["leader"]["life"], life);
    }
}

#[test]
fn emerald_maiden_counts_other_effective_wards_but_not_enemy_or_ex_cards() {
    for number in ["BP02-105", "BP02-P30"] {
        for silenced in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"guard","card":"BP01-046"},
                {"id":"orca","card":"BP02-T05"},
                {"id":"provider","card":"BP20-072","state":{"silenced":silenced}}
            ]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"BP01-046"}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-046"}]);
            let mut game = new_game(&initial);
            let play = game.play_internal_card("source", PLAYER).unwrap();
            assert!(matches!(finish(&mut game, play), CardPlay::Entered(_)));
            trigger(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["hp"],
                if silenced { 5_u8 } else { 6 }
            );
            assert_eq!(state["P1"]["pp"]["current"], 7_u8);
        }
    }
}

#[test]
fn storm_rage_counts_own_witch_followers_and_hits_each_enemy_in_one_resolution() {
    for count in [0_usize, 2] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP09-051"}]);
        let mut allies = (0..count)
            .map(|n| json!({"id":format!("witch{n}"),"card":"BP01-070"}))
            .collect::<Vec<_>>();
        allies.push(json!({"id":"non-witch","card":"BP01-037"}));
        initial["players"]["P1"]["zones"]["field"] = json!(allies);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ex-witch","card":"BP01-070"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}},
            {"id":"enemy-witch","card":"BP01-070","state":{"hp":10_u8,"max_hp":10_u8}}
        ]);
        let mut game = new_game(&initial);
        let result = game.play_internal_card("source", PLAYER).unwrap();
        resolved(finish(&mut game, result));
        let state = game.projection(View::Referee).unwrap();
        for id in ["enemy", "enemy-witch"] {
            assert_eq!(state["objects"][id]["damage"], count);
        }
        assert_eq!(state["P2"]["leader"]["life"], 20 - count);
        assert_eq!(state["objects"]["non-witch"]["damage"], 0_u8);
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    }
}
