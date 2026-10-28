use super::*;

#[test]
fn bp01_081_fanfare_reads_maximum_pp_after_paying_for_entry() {
    for seat in ["P1", "P2"] {
        for (maximum, expected) in [(6_u8, 2_i64), (7, 5)] {
            let opponent = if seat == "P1" { "P2" } else { "P1" };
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["pp"] = json!({"current":maximum,"max":maximum});
            initial["players"][opponent]["pp"] = json!({"current":10_u8,"max":10_u8});
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"mage","card":"BP01-081"}]);
            let mut game = game(&initial);
            assert!(matches!(
                game.play_internal_card(
                    "mage",
                    PlayerId {
                        seat: seat_number(seat)
                    }
                )
                .unwrap(),
                CardPlay::Entered(_)
            ));
            let question = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &question.domain else {
                panic!("expected fanfare");
            };
            assert_eq!(options.len(), 1);
            assert_eq!(options[0].card, "BP01-081");
            resolved(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[seat]["pp"]["current"], maximum - 2);
            assert_eq!(state["objects"]["mage"]["power"], expected);
            assert_eq!(state["objects"]["mage"]["hp"], expected);
        }
    }
}
