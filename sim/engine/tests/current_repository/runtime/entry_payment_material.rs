use super::*;

#[test]
fn phantom_blaster_dragon_evolves_after_paying_with_an_evolved_blaster_dark() {
    let mut initial = position_with_source("CP03-084", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"fee","card":"CP03-086","state":{"evolved":true,"evolved_with":"fee-form"}}
    ]);
    initial["players"]["P1"]["zones"]["evolution"] =
        json!([{"id":"fee-form","card":"CP03-SL19"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP03-085"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let play = choose_card_by_number(&mut game, play, "CP03-086");
    assert!(matches!(accept(&mut game, play), CardPlay::Entered(_)));
    checks(&mut game);
    let fanfare = resolve_pending_card(&mut game, "CP03-084");
    let fanfare = choose_card_by_number(&mut game, fanfare, "CP03-085");
    drop(accept(&mut game, fanfare));
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(after["P1"]["evolve_deck"], json!(["fee-form"]));
    assert_eq!(after["objects"]["fee-form"]["face_up"], true);
    assert_eq!(after["objects"]["source"]["evolved_with"], "form");
    assert_eq!(after["P1"]["pp"]["current"], 7_i64);
}
