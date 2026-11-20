use super::*;

#[test]
fn tanzanite_convictor_checks_current_friendly_life_for_its_fanfare() {
    let mut initial = position_with_source("BP10-097", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ally","card":"BP01-171","state":{"hp":7_u8,"max_hp":9_u8}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"low","card":"BP01-171","state":{"hp":4_u8,"max_hp":4_u8}},
        {"id":"high","card":"BP01-171","state":{"hp":8_u8,"max_hp":8_u8}}
    ]);
    let mut board = game(&initial);
    drop(resolved(enter_with_fanfare(&mut board, "BP10-097")));
    checks(&mut board);

    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["hp"], 7_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["low"]));
    assert_eq!(state["objects"]["high"]["hp"], 4_u8);
}
