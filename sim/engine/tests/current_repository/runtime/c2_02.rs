use super::*;

fn actor(seat: &str) -> PlayerId {
    PlayerId {
        seat: seat_number(seat),
    }
}

#[test]
fn angel_snipe_quick_targets_only_an_enemy_follower_and_deals_two_once() {
    for (seat, number) in [("P1", "BP01-179"), ("P2", "BP01-179")] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let mut initial = draw_position();
        initial["turn"]["active"] = json!(seat);
        initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":number}]);
        initial["players"][seat]["zones"]["field"] = json!([{"id":"ally","card":"CP03-016"}]);
        initial["players"][opponent]["zones"]["field"] = json!([{"id":"amulet","card":"BP19-108"}]);
        let mut no_target = game(&initial);
        let before_no_target = format!("{no_target:?}");
        no_target
            .play_internal_card("spell", actor(seat))
            .unwrap_err();
        assert_eq!(format!("{no_target:?}"), before_no_target);
        initial["players"][opponent]["zones"]["field"] = json!([
            {"id":"amulet","card":"BP19-108"}, {"id":"enemy","card":"CP03-016"}
        ]);
        for quick in [false, true] {
            let mut position = initial.clone();
            if quick {
                position["turn"]["active"] = json!(opponent);
            }
            let mut game = game(&position);
            let play = if quick {
                let before_window = format!("{game:?}");
                game.play_internal_card("spell", actor(seat)).unwrap_err();
                assert_eq!(format!("{game:?}"), before_window);
                game.internal_rule_action(&json!({"do":"end-phase"}))
                    .unwrap();
                let question = game.internal_quick_choice().unwrap().unwrap();
                assert_eq!(question.actor, actor(seat));
                let InputDomain::Quick { options, .. } = &question.domain else {
                    panic!("Quick window");
                };
                assert_eq!(options.len(), 1);
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Quick(Some(options[0].handle)),
                )
                .unwrap()
            } else {
                game.play_internal_card("spell", actor(seat)).unwrap()
            };
            let CardPlay::Awaiting(question) = play else {
                panic!("one enemy follower target");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("target domain");
            };
            assert_eq!(domain.members.len(), 1);
            assert_eq!(domain.members[0].card.as_deref(), Some("CP03-016"));
            let before = format!("{game:?}");
            game.answer_internal_card(
                actor(opponent),
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap_err();
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![99]))
                .unwrap_err();
            assert_eq!(format!("{game:?}"), before);
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|f| matches!(f, Fact::Damage(_)))
                    .count(),
                1
            );
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["objects"]["enemy"]["hp"], 1_i64);
            assert_eq!(state["objects"]["ally"]["hp"], 3_i64);
            assert_eq!(state[opponent]["leader"]["life"], 20_u64);
            assert_eq!(state[seat]["pp"]["current"], 9_u64);
            assert_eq!(state[seat]["cemetery"], json!(["spell"]));
        }
    }
}

#[test]
fn unmapped_reprint_keeps_the_current_runtime_guard() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"ETD03-019"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"CP03-016"}]);
    let mut game = game(&initial);
    let before = format!("{game:?}");
    let error = game.play_internal_card("spell", actor("P1")).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("current ability definitions unavailable")
    );
    assert_eq!(format!("{game:?}"), before);
}
