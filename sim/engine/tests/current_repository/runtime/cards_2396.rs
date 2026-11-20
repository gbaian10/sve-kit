use super::*;

#[test]
fn phantom_blaster_overlord_replaces_original_pp_with_its_named_follower() {
    let mut initial = position_with_source("CP03-083", "hand");
    initial["players"]["P1"]["pp"] = json!({"current": 1, "max": 1});
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"fee","card":"CP03-084"}, {"id":"other","card":"CP03-086"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards("paladin", "CP03-086", 15));
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    let materials = accept(&mut game, play);
    let entered = choose_card_by_number(&mut game, materials, "CP03-084");
    assert!(matches!(accept(&mut game, entered), CardPlay::Entered(_)));

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 1_i64);
    assert!(after["P1"]["cemetery"].as_array().unwrap().contains(&json!("fee")));
    assert_eq!(after["P1"]["field"], json!(["other", "source"]));
}
