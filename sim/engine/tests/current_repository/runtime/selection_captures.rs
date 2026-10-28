use super::*;

#[test]
fn bp20_073_captures_selected_life_and_damages_the_targets_controller() {
    for seat in ["P1", "P2"] {
        let other = if seat == "P1" { "P2" } else { "P1" };
        let actor = PlayerId {
            seat: seat_number(seat),
        };
        for (damage, leader_life) in [(0_u8, 20_u8), (2, 17)] {
            let mut initial = draw_position();
            initial["turn"]["active"] = json!(seat);
            initial["players"][seat]["zones"]["hand"] = json!([{"id":"spell","card":"BP20-073"}]);
            initial["players"][other]["zones"]["field"] =
                json!([{"id":"victim","card":"BP01-037","owner":seat,"state":{"damage":damage}}]);
            let mut board = game(&initial);
            let CardPlay::Awaiting(question) = board.play_internal_card("spell", actor).unwrap()
            else {
                panic!("target");
            };
            let result = resolved(
                board
                    .answer_internal_card(actor, question.token, Answer::Targets(vec![0]))
                    .unwrap(),
            );
            let state = board.projection(View::Referee).unwrap();
            assert_eq!(state[other]["leader"]["life"], leader_life);
            assert_eq!(state[seat]["leader"]["life"], 20_u8);
            assert!(
                result
                    .progress
                    .facts
                    .iter()
                    .any(|fact| matches!(fact, Fact::Damage(hit) if hit.amount.0.get() == 6))
            );
        }
    }
}
