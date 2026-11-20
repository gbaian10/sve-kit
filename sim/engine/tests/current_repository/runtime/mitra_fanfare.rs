use super::union_burst_batch1::{pending, question};
use super::*;

#[test]
fn mitra_resolves_the_turn_players_roll_and_draw_before_the_other_players_roll_and_discard() {
    let mut initial = position_with_source("BP06-110", "hand");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"draw-a","card":"BP01-171"},{"id":"draw-b","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"reroll","card":"ECP02-065"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"discard-a","card":"BP01-171"},{"id":"keep","card":"BP01-171"},
        {"id":"discard-b","card":"BP01-171"}
    ]);
    let mut board = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &json!({
            "dice": [2_u8, 5_u8],
            "random_selections": [{
                "n": 1_u8, "from": "P2.hand", "result": ["discard-a", "discard-b"]
            }]
        }),
        "mitra-fanfare",
    )
    .unwrap();
    assert!(matches!(
        board.play_internal_card("source", actor()).unwrap(),
        CardPlay::Entered(_)
    ));
    checks(&mut board);
    let reroll = question(pending(&mut board, "BP06-110"));
    assert_eq!(reroll.actor, PlayerId { seat: 2 });
    assert_eq!(
        reroll.domain,
        InputDomain::DiceReroll {
            value: Nat(5),
            remaining: Nat(1),
        }
    );
    let before = board.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["hand"], json!(["draw-a", "draw-b"]));
    assert_eq!(before["P2"]["cemetery"], json!([]));

    let result = resolved(
        board
            .answer_internal_card(reroll.actor, reroll.token, Answer::Integer(Nat(0)))
            .unwrap(),
    );
    let rolls = result.progress.facts.iter().filter_map(|fact| {
        let Fact::Dice(event) = fact else {
            return None;
        };
        Some((event.roller.seat, event.value))
    });
    assert!(rolls.eq([(1, DieFace::Two), (2, DieFace::Five)]));
    checks(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["hand"], json!(["keep"]));
    assert_eq!(state["P2"]["cemetery"], json!(["discard-a", "discard-b"]));
}
