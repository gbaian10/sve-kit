use super::*;

#[test]
fn sanae_all_printings_cap_each_event_instead_of_accumulating_a_turn_limit() {
    for number in ["CP02-101", "CP02-P83", "CSD02c-018"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"subject","card":number}]);
        let mut game = with_producer(&initial, damage());
        for total in [3_u8, 6_u8] {
            let result = strike_with_producer(&mut game);
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter_map(|fact| if let Fact::Damage(event) = fact {
                        Some(event.amount.0.get())
                    } else {
                        None
                    })
                    .collect::<Vec<_>>(),
                vec![3],
                "every four-point hit becomes three independently"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["subject"]["damage"],
                total,
                "separate capped hits accumulate actual damage"
            );
            game.resume_internal_card().unwrap();
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["field"]
                .as_array()
                .unwrap()
                .len(),
            2,
            "the real seven-life follower survives six actual damage beside the producer"
        );
    }
}
