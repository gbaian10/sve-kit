#![expect(clippy::panic, reason = "A missing trigger invalidates the scenario.")]
use super::*;

fn resolve_trigger(game: &mut Game, seat: &str, event: &str) -> Activation {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the printed trigger");
    };
    assert_eq!(
        question.actor.seat,
        seat_number(seat),
        "the controller chooses"
    );
    assert_eq!(options.len(), 1, "only the matching printed head triggers");
    assert_eq!(options[0].line, 1, "both heads keep the same printed line");
    assert!(
        options[0].event.contains(event),
        "the event matches the printed head"
    );
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(
        PlayerId {
            seat: if seat == "P1" { 2 } else { 1 },
        },
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap_err();
    assert_eq!(
        game.projection(View::Referee).unwrap(),
        before,
        "a wrong actor preserves the pending ability"
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

fn assert_fairies(game: &Game, seat: &str, count: usize) {
    let state = game.projection(View::Referee).unwrap();
    let ex = state[seat]["ex"].as_array().unwrap();
    assert_eq!(
        ex.len(),
        count,
        "each printed trigger creates exactly one Fairy"
    );
    for id in ex {
        let token = &state["objects"][id.as_str().unwrap()];
        assert_eq!(
            token["name"], "フェアリー",
            "the admitted Fairy token is created"
        );
        assert_eq!(token["power"], 1_u8, "the token keeps its printed power");
        assert_eq!(token["hp"], 1_u8, "the token keeps its printed health");
    }
}

#[test]
fn bp16_015_and_aliases_create_one_fairy_on_entry_and_one_on_departure() {
    for number in ["BP16-015", "EBD01-019", "PR-434"] {
        for seat in ["P1", "P2"] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["field"] = json!([]);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            let mut game = with_producer(
                &initial,
                Action::Destroy {
                    objects: subjects(),
                },
            );
            assert!(matches!(
                game.play_internal_card(
                    "source",
                    PlayerId {
                        seat: seat_number(seat)
                    }
                )
                .unwrap(),
                CardPlay::Entered(_)
            ));
            let entry = resolve_trigger(&mut game, seat, "EnterField");
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_fairies(&game, seat, 1);
            activate(&mut game, seat);
            game.resume_internal_card().unwrap();
            let exit = resolve_trigger(&mut game, seat, "LeaveField");
            assert_ne!(entry.source.definition, exit.source.definition);
            assert_eq!(entry.source.holder, exit.source.holder);
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            assert_fairies(&game, seat, 2);
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state[seat]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("source"))
            );
            assert_eq!(state[seat]["pp"]["current"], 9_u8);
        }
    }
}
