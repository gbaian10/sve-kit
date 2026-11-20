use super::*;

#[test]
fn magna_legacy_rounds_up_its_own_deck_and_uses_the_banish_result_for_damage() {
    let mut initial = position_with_source("BP05-SL06", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!(cards("deck-", "BP01-171", 29));
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"hp":9_u8,"max_hp":9_u8}}]);
    let mut board = game(&initial);
    drop(resolved(enter_with_fanfare(&mut board, "BP05-SL06")));
    checks(&mut board);

    let state = board.projection(View::Referee).unwrap();
    let banished = state["P1"]["banish"].as_array().unwrap();
    assert_eq!(banished.len(), 15);
    assert_eq!(banished[0], "deck-0");
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 14);
    assert_eq!(state["P2"]["leader"]["life"], 12_u8);
    assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
}
