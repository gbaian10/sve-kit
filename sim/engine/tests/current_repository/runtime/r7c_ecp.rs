#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "The complete Hishi procedures require their declared choices."
)]
use super::*;

const PRINTS: [(&str, &str); 4] = [
    ("ECP01-037", "ECP01-038"),
    ("ECP01-SL17", "ECP01-SL18"),
    ("ECP01-SP17", "ECP01-SP18"),
    ("ECP01-SSP17", "ECP01-SSP18"),
];
const PLAYER: PlayerId = PlayerId { seat: 1 };

fn initial(number: &str, zone: &str) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"][zone] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..3)
            .map(|i| json!({"id":format!("d{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    state
}

fn game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "r7c-ecp",
    )
    .unwrap()
}

fn checks(board: &mut Game) {
    while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        board.resume_internal_card().unwrap();
    }
}

fn pending(board: &mut Game) -> CardPlay {
    checks(board);
    let q = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed Hishi pending ability")
    };
    assert_eq!(
        options.len(),
        1,
        "only the printed Hishi ability is pending"
    );
    board
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap()
}

#[test]
fn hishi_all_printings_draw_for_nonempty_equal_successful_mill_results_only() {
    for (number, _) in PRINTS {
        for case in 0_u8..4 {
            let mut state = initial(number, "hand");
            let cards = match case {
                0 => vec!["BP01-171", "BP01-171", "BP01-171"],
                1 => vec!["BP01-171", "BP01-080", "BP01-171"],
                2 => vec!["BP01-171"],
                _ => vec![],
            };
            state["players"]["P1"]["zones"]["deck"] = json!(
                cards
                    .iter()
                    .enumerate()
                    .map(|(i, card)| json!({"id":format!("d{i}"),"card":card}))
                    .collect::<Vec<_>>()
            );
            state["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"old","card":"BP01-080"}]);
            let mut board = game(&state);
            assert!(
                matches!(
                    board.play_internal_card("source", PLAYER).unwrap(),
                    CardPlay::Entered(_)
                ),
                "the complete printed base enters before Fanfare"
            );
            let result = pending(&mut board);
            assert!(
                matches!(result, CardPlay::Resolved(_) | CardPlay::GameEnded(_)),
                "the declared Fanfare resolves or reaches the rules terminal"
            );
            checks(&mut board);
            let after = board.projection(View::Referee).unwrap();
            let moved = cards.len().min(2);
            assert_eq!(
                after["P1"]["cemetery"].as_array().unwrap().len(),
                moved.checked_add(1).unwrap()
            );
            assert_eq!(
                after["P1"]["hand"],
                if case == 0 { json!(["d2"]) } else { json!([]) }
            );
            assert_eq!(
                after["P1"]["deck"],
                if case == 1 { json!(["d2"]) } else { json!([]) }
            );
            assert_eq!(
                after["game"]["ended"],
                case == 2,
                "the single moved card satisfies the inferred condition and attempts an empty-deck draw"
            );
            if case == 2 {
                assert_eq!(
                    after["game"]["winner"], "P2",
                    "the existing draw-loss rule resolves the failed draw"
                );
            }
        }
    }
}

fn pay_evolution_trigger(board: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::OptionalCost => Answer::OptionalCost(true),
            InputDomain::CostMaterials(domain) => {
                assert_eq!(
                    domain.members.len(),
                    2,
                    "only the two Uma hand cards can pay the real evolve-time cost"
                );
                Answer::CostMaterials(domain.members.iter().map(|m| m.handle).collect())
            }
            other => panic!("unexpected printed evolve-time choice {other:?}"),
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    resolved(play);
    checks(board);
}

#[test]
fn hishi_all_printings_use_real_evolution_and_the_complete_existing_form() {
    for (number, form) in PRINTS {
        let mut state = initial(number, "field");
        state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        state["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"fuel0","card":"ECP01-035"},{"id":"fuel1","card":"ECP01-035"},{"id":"wrong","card":"BP01-080"}
        ]);
        let mut board = game(&state);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved",
            "the real base pays its printed evolution cost"
        );
        let play = pending(&mut board);
        pay_evolution_trigger(&mut board, play);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 9_u8);
        assert_eq!(after["P1"]["cemetery"], json!(["fuel0", "fuel1"]));
        assert_eq!(after["P1"]["hand"], json!(["wrong", "d0", "d1"]));
        assert_eq!(after["P1"]["deck"], json!(["d2"]));
    }
}

#[test]
fn hishi_all_printings_pay_the_real_meal_and_enter_the_race() {
    for (number, _) in PRINTS {
        let mut state = initial(number, "field");
        state["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot-0","card":"CSD01-032"}]);
        let mut board = game(&state);
        let activation = meal_race::feed(&mut board, number, 0, false);
        assert!(
            activation
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::Raced(_))),
            "the real Meal makes the follower enter the race"
        );
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 9_u8);
        assert_eq!(after["objects"]["carrot-0"]["face_up"], true);
    }
}
