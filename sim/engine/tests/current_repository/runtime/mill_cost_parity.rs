use super::*;

#[test]
fn miu_yaguchi_draws_if_the_milled_cards_printed_cost_is_odd() {
    let mut initial = position_with_source("CSD02c-P06", "hand");
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"odd","card":"CSD02c-009"},
        {"id":"next","card":"BP01-171"}
    ]);
    let mut board = game(&initial);
    drop(resolved(enter_with_fanfare(&mut board, "CSD02c-P06")));
    checks(&mut board);

    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["odd"]));
    assert_eq!(state["P1"]["hand"], json!(["next"]));
    assert_eq!(state["P1"]["leader"]["life"], 15_u8);
}
