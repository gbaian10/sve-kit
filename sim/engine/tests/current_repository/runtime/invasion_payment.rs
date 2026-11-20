use super::*;

#[test]
fn corpses_paid_to_each_invasion_mode_supply_their_own_attack() {
    let mut initial = position_with_source("BP21-090", "hand");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"nightmare","card":"BP01-117"},
        {"id":"other","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"first","card":"BP03-055"},
        {"id":"second","card":"BP05-057"}
    ]);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", PlayerId { seat: 1 }).unwrap();
    let mode = await_card_choice(play);
    let play = game
        .answer_internal_card(mode.actor, mode.token, Answer::Modes(vec![ModeId(1), ModeId(2)]))
        .unwrap();
    let play = accept(&mut game, play);
    let play = choose_card_by_number(&mut game, play, "BP03-055");
    let play = choose_card_by_number(&mut game, play, "BP05-057");
    let play = choose_card_by_number(&mut game, play, "BP01-117");
    let play = choose_card_by_number(&mut game, play, "BP01-171");
    drop(accept(&mut game, play));
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["first"]["damage"], 4);
    assert_eq!(after["objects"]["second"]["damage"], 2);
    assert_eq!(after["P1"]["banish"], json!(["nightmare", "other"]));
}
