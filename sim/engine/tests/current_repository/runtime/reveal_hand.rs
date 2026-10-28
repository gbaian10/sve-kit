use super::*;
use sve_engine::game::internal::InstructionOutcome;

#[test]
fn bp06_118_reveals_one_opponents_whole_hand_without_moving_or_retaining_locations() {
    for seat in ["P1", "P2"] {
        let opponent = if seat == "P1" { "P2" } else { "P1" };
        let view = if seat == "P1" { View::P1 } else { View::P2 };
        for count in [0_u64, 2] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] =
                json!([{"id":"detective","card":"BP06-118"}]);
            initial["players"][opponent]["zones"]["hand"] = json!((0..count).map(|n| json!({"id":format!("hidden{n}"),"card":if n == 0 {"BP11-111"} else {"BP01-085"}})).collect::<Vec<_>>());
            let mut game = game(&initial);
            assert!(game.projection(view).unwrap()["objects"]["hidden0"].is_null());
            assert!(matches!(
                game.play_internal_card(
                    "detective",
                    PlayerId {
                        seat: seat_number(seat)
                    }
                )
                .unwrap(),
                CardPlay::Entered(_)
            ));
            let before = game.projection(View::Referee).unwrap();
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("pending Fanfare");
            };
            let [option] = options.as_slice() else {
                panic!("one printed Fanfare");
            };
            assert_eq!(option.card, "BP06-118");
            let unchanged = format!("{game:?}");
            game.answer_internal_card(
                PlayerId {
                    seat: if question.actor.seat == 1 { 2 } else { 1 },
                },
                question.token,
                Answer::Pending(option.handle),
            )
            .unwrap_err();
            assert_eq!(format!("{game:?}"), unchanged);
            let result = resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(option.handle),
                )
                .unwrap(),
            );
            let revealed = result
                .progress
                .receipts
                .values()
                .find_map(|receipt| {
                    if let InstructionOutcome::Revealed(objects) = receipt {
                        Some(objects)
                    } else {
                        None
                    }
                })
                .unwrap();
            assert_eq!(u64::try_from(revealed.len()).unwrap(), count);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(before[opponent]["hand"], after[opponent]["hand"]);
            assert_eq!(after["objects"]["detective"]["power"], 3_u8);
            assert_eq!(after["objects"]["detective"]["hp"], 4_u8);
            assert_eq!(after[seat]["pp"]["current"], 7_u8);
            let observer = game.projection(view).unwrap();
            for n in 0..count {
                let id = format!("hidden{n}");
                assert_eq!(before["objects"][&id], after["objects"][&id]);
                assert!(
                    observer["objects"][&id].is_null(),
                    "resolution ended: no current opponent hand slot"
                );
                assert!(
                    observer["known_cards"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|card| card["id"] == id),
                    "the disclosed printed card remains historical knowledge"
                );
            }
            let completed = format!("{game:?}");
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(option.handle),
            )
            .unwrap_err();
            assert_eq!(format!("{game:?}"), completed);
        }
    }
}
