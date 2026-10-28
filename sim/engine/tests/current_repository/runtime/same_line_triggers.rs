#![expect(
    clippy::panic,
    reason = "A missing trigger domain means the game fixture cannot continue."
)]

use super::*;

fn resolve_singer(game: &mut Game, seat: &str, event: &str) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("singer trigger");
    };
    assert_eq!(
        question.actor.seat,
        seat_number(seat),
        "the controller chooses"
    );
    assert_eq!(options.len(), 1, "only this event's ability triggers");
    assert_eq!(options[0].card, "BP04-123", "the real singer triggers");
    assert_eq!(options[0].line, 1, "both abilities keep their printed line");
    assert!(
        options[0].event.contains(event),
        "the matching event caused the trigger"
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}

#[test]
fn bp04_123_same_line_fanfare_and_lastword_each_draw_once() {
    for seat in ["P1", "P2"] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["field"] = json!([]);
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"singer","card":"BP04-123"}]);
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":"entry-draw","card":"BP01-037"},
            {"id":"exit-draw","card":"BP01-037"}
        ]);
        let mut game = with_producer(
            &initial,
            Action::Destroy {
                objects: subjects(),
            },
        );
        assert!(matches!(
            game.play_internal_card(
                "singer",
                PlayerId {
                    seat: seat_number(seat)
                }
            )
            .unwrap(),
            CardPlay::Entered(_)
        ));
        let entry = resolve_singer(&mut game, seat, "EnterField");
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        let entry_state = game.projection(View::Referee).unwrap();
        assert_eq!(entry_state[seat]["hand"], json!(["entry-draw"]));
        assert_eq!(entry_state[seat]["deck"], json!(["exit-draw"]));
        activate(&mut game, seat);
        game.resume_internal_card().unwrap();
        let exit = resolve_singer(&mut game, seat, "LeaveField");
        assert_ne!(entry.source.definition, exit.source.definition);
        assert_eq!(entry.source.holder, exit.source.holder);
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        let exit_state = game.projection(View::Referee).unwrap();
        assert_eq!(exit_state[seat]["hand"], json!(["entry-draw", "exit-draw"]));
        assert_eq!(exit_state[seat]["deck"], json!([]));
        assert!(
            exit_state[seat]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("singer"))
        );
    }
}
