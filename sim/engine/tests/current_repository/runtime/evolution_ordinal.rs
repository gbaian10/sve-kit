#![expect(
    clippy::panic,
    reason = "The ordinal probe requires the real Fencer chain's declared pending and target domains."
)]
use super::event_cemetery::finish_checks;
use super::*;
use sve_engine::game::internal::DecisionView;

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":1_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"kyou","card":number},
        {"id":"first","card":"BP03-020"},{"id":"second","card":"BP03-020"},
        {"id":"third","card":"BP03-020"},{"id":"mach","card":"BP03-028"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"form-first","card":"BP03-021"},{"id":"form-second","card":"BP03-021"},
        {"id":"form-third","card":"BP03-021"},{"id":"form-mach","card":"BP03-029"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"hero-a","card":"BP03-036"},{"id":"hero-b","card":"BP03-036"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"draw0","card":"BP01-171"},{"id":"draw1","card":"BP01-171"},{"id":"draw2","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"CP03-013","state":{"power":0_u8,"hp":10_u8,"max_hp":10_u8}}
    ]);
    initial
}
fn game(number: &str) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        &position(number),
        &Value::Null,
        &Value::Null,
        "real-evolution-ordinal-red",
    )
    .unwrap()
}
fn pending(board: &mut Game, number: &str) -> CardPlay {
    pending_at(board, number, 0)
}
fn pending_at(board: &mut Game, number: &str, index: usize) -> CardPlay {
    finish_checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("real pending ability")
    };
    let entry = options
        .iter()
        .filter(|entry| entry.card == number)
        .nth(index)
        .unwrap();
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(entry.handle))
        .unwrap()
}
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("the real effect evolution or Mach target")
    };
    q
}
fn choose(board: &mut Game, q: &DecisionView, number: Option<&str>) -> CardPlay {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("printed target or private evolution resource")
    };
    let handles = number
        .map(|number| {
            vec![
                domain
                    .members
                    .iter()
                    .find(|member| member.card.as_deref() == Some(number))
                    .unwrap()
                    .handle,
            ]
        })
        .unwrap_or_default();
    board
        .answer_internal_card(q.actor, q.token, Answer::Targets(handles))
        .unwrap()
}
fn start(board: &mut Game) {
    assert_eq!(board.internal_rule_action(&json!({"do":"evolve","source":"first","evolve_card":"form-first","pay":{"pp":1_u8}})).unwrap().outcome,"resolved","the first real Fencer normally evolves and pays its printed cost");
}
fn follow_fencer(board: &mut Game, target: &str, form: Option<&str>) {
    let target_choice = awaiting(pending(board, "BP03-021"));
    let resource_choice = awaiting(choose(board, &target_choice, Some(target)));
    assert!(
        board.internal_decision(View::P2).is_none(),
        "the opponent cannot inspect the private evolution resource"
    );
    resolved(choose(board, &resource_choice, form));
    finish_checks(board);
}
fn storm(view: &Value) -> bool {
    view["objects"]["kyou"]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!("疾走"))
}

#[test]
fn a_single_real_evolution_rewards_the_first_occurrence_before_any_later_evolution() {
    for number in ["BP18-003", "BP18-SL03"] {
        let mut board = game(number);
        start(&mut board);
        resolved(pending(&mut board, number));
        finish_checks(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["hand"],
            json!(["draw0"]),
            "the first actual evolution draws exactly one"
        );
        assert_eq!(
            view["P2"]["leader"]["life"], 19_u8,
            "the first occurrence deals its unconditional one"
        );
        assert_eq!(
            view["objects"]["kyou"]["power"], 2_u8,
            "the first evolution has no third-event buff"
        );
        assert!(
            !storm(&view),
            "the first evolution has no second-event keyword"
        );
        follow_fencer(&mut board, "BP03-020", None);
        assert!(
            board.internal_pending_choice().unwrap().is_none(),
            "declining the true helper's optional evolution finishes the baseline without another event"
        );
    }
}

#[test]
fn three_real_evolutions_before_pending_resolution_still_reward_first_second_and_third_once_each() {
    for number in ["BP18-003", "BP18-SL03"] {
        let mut board = game(number);
        start(&mut board);
        follow_fencer(&mut board, "BP03-020", Some("BP03-021"));
        follow_fencer(&mut board, "BP03-028", Some("BP03-029"));
        let damage_choice = awaiting(pending(&mut board, "BP03-029"));
        resolved(choose(&mut board, &damage_choice, Some("CP03-013")));
        finish_checks(&mut board);
        let produced = board.projection(View::Referee).unwrap();
        for id in ["first", "second", "mach"] {
            assert_eq!(
                produced["objects"][id]["evolved"], true,
                "the real printed chain really evolves each follower"
            );
        }
        assert_eq!(
            produced["objects"]["enemy"]["hp"], 6_u8,
            "the complete real Mach Knight trigger finishes before Kyou"
        );
        assert!(
            produced["P1"]["hand"].as_array().unwrap().is_empty(),
            "no Kyou occurrence has resolved yet"
        );
        assert_eq!(
            produced["P1"]["pp"]["current"], 0_u8,
            "only the first ordinary evolution spends PP"
        );
        let q = board.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("three actual Kyou occurrences")
        };
        assert_eq!(
            options.len(),
            3,
            "one occurrence was collected for each of the three real evolutions"
        );
        assert!(
            options.iter().all(|entry| entry.card == number),
            "all remaining occurrences belong to complete Kyou"
        );
        for _ in 0_u8..3 {
            resolved(pending(&mut board, number));
            finish_checks(&mut board);
        }
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(
            after["P2"]["leader"]["life"], 17_u8,
            "every actual occurrence retains its unconditional damage"
        );
        assert_eq!(
            after["P1"]["hand"],
            json!(["draw0"]),
            "the first event must retain its draw after later evolutions; Kyou power={}, Storm={}",
            after["objects"]["kyou"]["power"],
            storm(&after)
        );
        assert!(
            storm(&after),
            "the second event must retain Storm after the third evolution"
        );
        for stat in ["power", "hp"] {
            assert_eq!(
                after["objects"]["kyou"][stat], 4_u8,
                "only the third event grants plus two to each stat"
            );
        }
    }
}

#[test]
fn fourth_real_evolution_adds_only_damage_and_reverse_pending_order_keeps_each_reward_once() {
    for number in ["BP18-003", "BP18-SL03"] {
        let mut board = game(number);
        start(&mut board);
        follow_fencer(&mut board, "BP03-020", Some("BP03-021"));
        follow_fencer(&mut board, "BP03-020", Some("BP03-021"));
        follow_fencer(&mut board, "BP03-028", Some("BP03-029"));
        let damage = awaiting(pending(&mut board, "BP03-029"));
        resolved(choose(&mut board, &damage, Some("CP03-013")));
        finish_checks(&mut board);
        for index in [3_usize, 2, 1, 0] {
            resolved(pending_at(&mut board, number, index));
            finish_checks(&mut board);
            let view = board.projection(View::Referee).unwrap();
            assert_eq!(
                view["P2"]["leader"]["life"],
                16 + index,
                "one damage for each resolved real occurrence"
            );
            assert_eq!(
                view["P1"]["hand"].as_array().unwrap().len(),
                usize::from(index == 0),
                "only the first evolution draws, even when resolved last"
            );
            assert_eq!(
                storm(&view),
                index <= 1,
                "only the second evolution grants Storm"
            );
            assert_eq!(
                view["objects"]["kyou"]["power"],
                if index == 3 { 2_u8 } else { 4_u8 },
                "the fourth evolution must not repeat the third reward"
            );
        }
    }
}

#[test]
fn a_real_super_evolution_is_one_first_evolution_for_both_kyou_prints() {
    for number in ["BP18-003", "BP18-SL03"] {
        let mut initial = position(number);
        initial["players"]["P1"]["sep"] = json!(1_u8);
        initial["turn"]["elapsed_turns"]["P1"] = json!(7_u8);
        let mut board = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "real-kyou-super",
        )
        .unwrap();
        assert_eq!(board.internal_rule_action(&json!({"do":"evolve","source":"first","evolve_card":"form-first","pay":{"pp":1_u8,"ep":0_u8,"sep":1_u8}})).unwrap().outcome,"resolved");
        resolved(pending(&mut board, number));
        finish_checks(&mut board);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(
            view["P1"]["sep"], 0_u8,
            "the real rule procedure spends the super-evolution resource"
        );
        assert_eq!(
            view["P1"]["hand"],
            json!(["draw0"]),
            "super evolution is exactly one first evolution"
        );
        assert_eq!(view["P2"]["leader"]["life"], 19_u8);
        assert!(!storm(&view));
        assert_eq!(view["objects"]["kyou"]["power"], 2_u8);
        follow_fencer(&mut board, "BP03-020", None);
    }
}
