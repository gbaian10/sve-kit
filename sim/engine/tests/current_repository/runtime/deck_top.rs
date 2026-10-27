use super::*;

#[test]
fn bp01_120_moves_top_two_as_available_without_drawing_or_revealing_the_remainder() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        for count in [0_u64, 1, 3] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] =
                json!([{"id":"sergeant","card":"BP01-120"}]);
            initial["players"][seat]["zones"]["deck"] = json!(
                (0..count)
                    .map(|n| json!({"id":format!("deck{n}"),"card":"BP01-037"}))
                    .collect::<Vec<_>>()
            );
            initial["players"][opponent]["zones"]["deck"] =
                json!([{"id":"other","card":"BP01-037"}]);
            let mut game = game(&initial);
            assert!(matches!(
                game.play_internal_card(
                    "sergeant",
                    PlayerId {
                        seat: seat_number(seat)
                    }
                )
                .unwrap(),
                CardPlay::Entered(_)
            ));
            let pending = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &pending.domain else {
                panic!("Fanfare");
            };
            let result = resolved(
                game.answer_internal_card(
                    pending.actor,
                    pending.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            let moved = result
                .progress
                .facts
                .iter()
                .filter_map(|fact| {
                    if let Fact::Movement(movement) = fact {
                        Some(movement)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(moved.len(), usize::try_from(count.min(2)).unwrap());
            for fact in moved {
                assert_eq!(fact.from.kind, ZoneKind::Deck);
                assert_eq!(fact.method, MoveMethod::Put);
                assert_eq!(fact.movement.actual_to.unwrap().kind, ZoneKind::Cemetery);
                assert_ne!(
                    fact.movement.before.generation,
                    fact.movement.after.unwrap().generation
                );
            }
            assert!(game.internal_decision(View::Referee).is_none());
            game.resume_internal_card().unwrap();
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state[seat]["cemetery"],
                json!(
                    (0..count.min(2))
                        .map(|n| format!("deck{n}"))
                        .collect::<Vec<_>>()
                )
            );
            assert_eq!(
                state[seat]["deck"],
                json!((2..count).map(|n| format!("deck{n}")).collect::<Vec<_>>())
            );
            assert_eq!(state[seat]["hand"], json!([]));
            assert_eq!(state[seat]["pp"]["current"], 8_u8);
            assert_eq!(state[opponent]["deck"], json!(["other"]));
            assert_eq!(
                state["game"]["ended"], false,
                "moving from an empty deck is not a failed draw"
            );
            for view in [View::P1, View::P2] {
                let public = game.projection(view).unwrap();
                assert!(public["objects"].get("other").is_none());
                assert!(public["objects"].get("deck2").is_none());
                if count > 0 {
                    assert_eq!(public["objects"]["deck0"]["card"], "BP01-037");
                }
            }
        }
    }
}
