use super::*;

#[test]
fn bp01_097_fanfare_chooses_after_entry_and_keeps_entry_when_no_enemy_exists() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for enemy in [false, true] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"dragon","card":"BP01-097"}]);
            if enemy {
                initial["players"][opponent]["zones"]["field"] =
                    json!([{"id":"enemy","card":"BP01-037"}]);
            }
            let mut game = game(&initial);
            let entered = game
                .play_internal_card(
                    "dragon",
                    PlayerId {
                        seat: seat_number(seat),
                    },
                )
                .unwrap();
            assert!(
                matches!(entered, CardPlay::Entered(_)),
                "entry has no play-time targets"
            );
            assert!(game.internal_decision(View::Referee).is_none());
            let entered_state = game.projection(View::Referee).unwrap();
            assert_eq!(entered_state[seat]["field"], json!(["dragon"]));
            assert_eq!(entered_state[seat]["pp"]["current"], 3_u8);
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("pending Fanfare")
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].card, "BP01-097");
            let CardPlay::Awaiting(question) = game
                .answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap()
            else {
                panic!("resolution field choice");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("field choice")
            };
            assert_eq!(domain.members.len(), usize::from(enemy));
            assert_eq!(domain.cardinality.minimum, Nat(u64::from(enemy)));
            let before = format!("{game:?}");
            game.answer_internal_card(
                PlayerId {
                    seat: seat_number(opponent),
                },
                question.token,
                Answer::Targets(vec![]),
            )
            .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "another actor cannot change the pending choice"
            );
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(if enemy { vec![0] } else { vec![] }),
                )
                .unwrap(),
            );
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::Damage(_)))
                    .count(),
                usize::from(enemy)
            );
            if enemy {
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                    7_u8
                );
            }
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seat]["field"], json!(["dragon"]));
            assert_eq!(state[seat]["pp"]["current"], 3_u8);
            if enemy {
                assert_eq!(state[opponent]["cemetery"], json!(["enemy"]));
            }
        }
    }
}
