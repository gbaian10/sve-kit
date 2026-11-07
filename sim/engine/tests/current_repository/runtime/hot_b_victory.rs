// The shared repository selection is loaded once; cold source preparation dominates these cases.
#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed victory scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn actual(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "printed-victory",
    )
    .unwrap()
}

fn attack_leader(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    finish_checks(board);
    if let Some(quick) = board.internal_quick_choice().unwrap() {
        assert!(
            matches!(
                board
                    .answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                    .unwrap(),
                CardPlay::Continued(_)
            ),
            "Passing Quick must resume the attack."
        );
    }
}

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":number,"state":{"entered_this_turn":false}}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial
}

fn trigger(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending ability")
    };
    let handle = options
        .iter()
        .find(|o| o.card == number && o.line == line)
        .unwrap()
        .handle;
    board
        .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}

#[test]
fn spartacus_all_printings_use_actual_discards_and_check_victory_before_later_triggers() {
    for number in ["BP09-020", "BP09-SL06", "BP09-U02"] {
        for (deck, discard, delayed) in [(0_u8, 0_usize, false), (4, 2, false), (5, 2, true)] {
            let mut initial = position(number);
            initial["players"]["P1"]["zones"]["deck"] = json!(
                (0..deck)
                    .map(|n| json!({"id":format!("d-{n}"),"card":"BP01-171"}))
                    .collect::<Vec<_>>()
            );
            initial["players"]["P1"]["zones"]["hand"] = json!([
                {"id":"discard-a","card":if discard > 0 {"BP01-170"} else {"BP01-171"}},
                {"id":"discard-b","card":"BP01-171"}
            ]);
            let mut board = actual(&initial);
            board
                .internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            let CardPlay::Awaiting(question) = trigger(&mut board, number, 2) else {
                panic!("any-number discard")
            };
            assert!(board.internal_decision(View::P2).is_none());
            let result = board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets((0..u64::try_from(discard).unwrap()).collect()),
                )
                .unwrap();
            if delayed {
                resolved(result);
            } else {
                assert!(matches!(result, CardPlay::GameEnded(_)));
            }
            let view = board.projection(View::Referee).unwrap();
            if delayed {
                assert_ne!(view["game"]["ended"], true);
                assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), 2);
                resolved(trigger(&mut board, "BP01-170", 1));
                finish_checks(&mut board);
                let after_draw = board.projection(View::Referee).unwrap();
                assert_eq!(after_draw["P1"]["deck"].as_array().unwrap().len(), 1);
                assert_ne!(
                    after_draw["game"]["ended"], true,
                    "Q1480 does not recheck Spartacus after another ability"
                );
            } else {
                assert_eq!(view["game"]["winner"], "P1");
                assert_eq!(view["game"]["by"], "1.2.4");
                assert!(board.internal_pending_choice().unwrap().is_none());
            }
        }
    }
}

#[test]
fn ancient_forest_base_all_printings_win_only_after_positive_attack_damage_to_leader() {
    for number in ["BP04-003", "BP04-SL02"] {
        for power in [0_u8, 8] {
            let mut initial = position(number);
            initial["players"]["P1"]["zones"]["field"][0]["state"]["power"] = json!(power);
            let mut board = actual(&initial);
            attack_leader(&mut board);
            if power == 0 {
                finish_checks(&mut board);
                assert!(
                    board.internal_pending_choice().unwrap().is_none(),
                    "Q558 requires actual damage"
                );
                assert_ne!(
                    board.projection(View::Referee).unwrap()["game"]["ended"],
                    true
                );
            } else {
                assert!(matches!(
                    trigger(&mut board, number, 2),
                    CardPlay::GameEnded(_)
                ));
                assert_eq!(
                    board.projection(View::Referee).unwrap()["game"]["winner"],
                    "P1"
                );
            }
        }
    }
}

#[test]
fn ancient_forest_evolution_all_printings_bottom_the_enemy_before_winning_by_attack() {
    for (base, form) in [("BP04-003", "BP04-004"), ("BP04-SL02", "BP04-SL03")] {
        let mut initial = position(base);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":form,"face_up":false}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"tail","card":"BP01-171"}]);
        let mut board = actual(&initial);
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
        let CardPlay::Awaiting(question) = trigger(&mut board, form, 2) else {
            panic!("evolution target")
        };
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("formal follower target")
        };
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members.first().unwrap().handle]),
                )
                .unwrap(),
        );
        finish_checks(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P2"]["field"], json!([]));
        assert_eq!(view["P2"]["deck"], json!(["tail", "enemy"]));
        attack_leader(&mut board);
        assert!(matches!(
            trigger(&mut board, form, 1),
            CardPlay::GameEnded(_)
        ));
        assert_eq!(
            board.projection(View::Referee).unwrap()["game"]["winner"],
            "P1"
        );
    }
}
