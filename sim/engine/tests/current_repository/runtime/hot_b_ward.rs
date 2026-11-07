use super::event_cemetery::finish_checks;
use super::*;

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":number,"state":{"entered_this_turn":true}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"guard","card":"BP01-T09","state":{"acted":true}}]);
    initial
}

#[expect(
    clippy::panic,
    reason = "The printed evolution and end-phase triggers must be offered."
)]
fn trigger(board: &mut Game, number: &str) {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger");
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    resolved(
        board
            .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
            .unwrap(),
    );
    finish_checks(board);
}

fn attack(board: &mut Game) -> String {
    board
        .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap()
        .outcome
}

#[test]
fn evolved_justice_all_printings_attack_past_ward_on_entry() {
    for number in ["BP10-055", "BP10-SL13", "SD08-004"] {
        let mut board = game(&position(number));
        assert_eq!(attack(&mut board), "resolved");
        let view = board.projection(View::Referee).unwrap();
        assert!(
            view["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("疾走"))
        );
        assert!(
            view["objects"]["guard"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護"))
        );
    }
}

#[test]
fn elephant_evolution_counts_own_hand_and_attacks_past_ward() {
    for number in ["BP04-006", "PR-142"] {
        let mut initial = position("BP04-005");
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":number,"face_up":false}]);
        initial["players"]["P1"]["zones"]["hand"] = json!(
            (0..3_u8)
                .map(|n| json!({"id":format!("mine-{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["hand"] = json!(
            (0..2_u8)
                .map(|n| json!({"id":format!("theirs-{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut board = game(&initial);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":4_u8}})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        trigger(&mut board, number);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["source"]["power"], 4_u8);
        assert_eq!(view["objects"]["source"]["hp"], 4_u8);
        assert_eq!(attack(&mut board), "resolved");
    }
}

#[test]
fn lindworm_faces_keep_white_end_phase_and_black_ward_bypass_separate() {
    for number in ["BP09-056", "BP09-P17"] {
        for face in [0_u8, 1] {
            let mut initial = position(number);
            initial["players"]["P1"]["zones"]["field"][0]["state"] =
                json!({"face":face,"entered_this_turn":false});
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["deck"] = json!(
                (0..4_u8)
                    .map(|n| json!({"id":format!("d-{n}"),"card":"BP01-171"}))
                    .collect::<Vec<_>>()
            );
            let mut board = game(&initial);
            assert_eq!(
                attack(&mut board),
                if face == 1 {
                    "resolved"
                } else {
                    "cannot-attack"
                }
            );
            if face == 0 {
                assert_eq!(
                    board
                        .internal_rule_action(&json!({"do":"end-phase"}))
                        .unwrap()
                        .outcome,
                    "resolved"
                );
                trigger(&mut board, number);
                let view = board.projection(View::Referee).unwrap();
                assert_eq!(view["P1"]["leader"]["life"], 16_u8);
                assert_eq!(view["P1"]["hand"], json!(["d-0", "d-1", "d-2"]));
                assert_eq!(view["P1"]["deck"], json!(["d-3"]));
                assert!(
                    view["objects"]["source"]["keywords"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("守護"))
                );
            }
        }
    }
}
