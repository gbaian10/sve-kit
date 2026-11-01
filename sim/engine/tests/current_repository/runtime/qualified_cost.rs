#![expect(
    clippy::panic,
    reason = "Unexpected choices invalidate printed payment scenarios."
)]
use super::*;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial
}

fn play(game: &mut Game) {
    let mut result = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    while let CardPlay::Awaiting(question) = result {
        let InputDomain::WardEntry { .. } = question.domain else {
            panic!("printed Ward entry");
        };
        result = game
            .answer_internal_card(question.actor, question.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(
        matches!(result, CardPlay::Entered(_)),
        "the printed follower enters after normal payment"
    );
}

#[test]
fn bp01_061_uses_the_highest_sc_interval() {
    for (count, remaining) in [(4_usize, 1_u8), (5, 4), (9, 4), (10, 7), (14, 7), (15, 10)] {
        let mut initial = position("BP01-061");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0..count)
                .map(|i| json!({"id":format!("spell-{i}"),"card":"SD03-005"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            remaining
        );
    }
}

#[test]
fn cp04_067_is_free_only_at_seven_maximum_pp_and_retains_ward() {
    for maximum in [6_u8, 7] {
        let mut initial = position("CP04-067");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "qualified-title-cost",
        )
        .unwrap();
        play(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if maximum == 7 { 7_u8 } else { 4_u8 }
        );
        assert!(
            state["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            "the discounted follower enters its controllers field"
        );
        assert!(
            state["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("守護")),
            "the discount preserves printed Ward"
        );
    }
}

#[test]
fn bp03_097_discounts_at_five_life_and_keeps_its_printed_stats() {
    for life in [5_u8, 6] {
        let mut initial = position("BP03-097");
        initial["players"]["P1"]["leader"]["life"] = json!(life);
        let mut game = game(&initial);
        play(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if life == 5 { 9_u8 } else { 4_u8 }
        );
        assert_eq!(state["objects"]["source"]["power"], 6_u8);
    }
}
