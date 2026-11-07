// These scenarios share the real repository fixture, whose initial load dominates cold runs.
#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed movement scenarios."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0..6_u8)
            .map(|n| json!({"id":format!("d-{n}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial
}

fn trigger(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger");
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

fn finish(board: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("printed formal target");
        };
        let count = usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap();
        let handles = domain
            .members
            .iter()
            .take(count)
            .map(|member| member.handle)
            .collect();
        play = board
            .answer_internal_card(question.actor, question.token, Answer::Targets(handles))
            .unwrap();
    }
    assert!(
        matches!(play, CardPlay::Resolved(_)),
        "complete printed ability"
    );
}

#[test]
fn dragon_blader_real_evolution_can_banish_either_players_ex_card() {
    for seat in ["P1", "P2"] {
        let mut initial = position("BP06-068");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP06-068"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"BP06-069","face_up":false}]);
        initial["players"][seat]["zones"]["ex"] = json!([{"id":"chosen","card":"BP01-037"}]);
        let mut board = game(&initial);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        let play = trigger(&mut board, "BP06-069", 1);
        finish(&mut board, play);
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view[seat]["ex"], json!([]));
        assert_eq!(view[seat]["banish"], json!(["chosen"]));
    }
}

#[test]
fn skull_juggler_all_printings_require_two_pale_moon_cards_then_draw_one() {
    for number in ["CP03-061", "CP03-P43"] {
        for available in [1_u8, 2] {
            let mut initial = position(number);
            initial["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..available)
                    .map(|n| json!({"id":format!("p-{n}"),"card":"CP03-052"}))
                    .collect::<Vec<_>>()
            );
            let mut board = game(&initial);
            if available == 1 {
                let before = format!("{board:?}");
                board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap_err();
                assert_eq!(
                    format!("{board:?}"),
                    before,
                    "cannot play without two targets"
                );
            } else {
                let play = board
                    .play_internal_card("source", PlayerId { seat: 1 })
                    .unwrap();
                finish(&mut board, play);
                let view = board.projection(View::Referee).unwrap();
                assert_eq!(view["P1"]["banish"], json!(["p-0", "p-1"]));
                assert_eq!(view["P1"]["hand"], json!(["d-0"]));
                assert_eq!(view["P1"]["cemetery"], json!(["source"]));
            }
        }
    }
}
