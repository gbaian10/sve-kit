#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
use super::*;

fn declare(game: &mut Game) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger")
    };
    assert_eq!(options.len(), 1, "one printed Fanfare is pending");
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(options[0].handle),
    )
    .unwrap()
}

fn position(card: &str, matches: usize, owner: &str, zone: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{ "id":"source", "card":card }]);
    initial["players"][owner]["zones"][zone] = json!(
        (0..matches)
            .map(|i| json!({"id":format!("match-{i}"),"card":card}))
            .collect::<Vec<_>>()
    );
    if let Some(cards) = initial["players"]["P1"]["zones"]["cemetery"].as_array_mut() {
        cards.push(json!({"id":"unrelated","card":"BP01-037"}));
    }
    initial
}

#[test]
fn filtered_count_bp03_036_checks_only_allied_cemetery_heroes() {
    for (count, owner, zone, qualifies) in [
        (1, "P1", "cemetery", false),
        (2, "P1", "cemetery", true),
        (2, "P2", "cemetery", false),
        (2, "P1", "ex", false),
    ] {
        let mut game = game(&position("BP03-036", count, owner, zone));
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        resolved(declare(&mut game));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["hp"],
            if qualifies { 3_u8 } else { 2_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["keywords"],
            if qualifies {
                json!(["守護"])
            } else {
                json!([])
            }
        );
    }
}

#[test]
fn filtered_count_bp05_012_heals_at_three_hunters_without_spending_them() {
    for count in [2, 3] {
        let mut game = game(&position("BP05-012", count, "P1", "cemetery"));
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        resolved(declare(&mut game));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["leader"]["life"],
            if count == 3 { 12_u8 } else { 10_u8 }
        );
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), count + 1);
    }
}

#[test]
fn filtered_count_bp21_088_recovers_pp_only_for_five_original_cost_two_cards() {
    for count in [4, 5] {
        let mut initial = position("BP21-088", count, "P1", "cemetery");
        initial["players"]["P1"]["zones"]["cemetery"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"wrong-cost","card":"BP05-012"}));
        let mut game = game(&initial);
        assert!(matches!(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        resolved(declare(&mut game));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["pp"]["current"],
            if count == 5 { 10_u8 } else { 8_u8 }
        );
    }
}
