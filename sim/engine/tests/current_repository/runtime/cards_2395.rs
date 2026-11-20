use super::*;

#[test]
fn jabberwock_uses_the_paid_followers_original_cost_as_a_strict_lower_bound() {
    let mut initial = position_with_source("BP03-055", "hand");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"material","card":"BP03-056"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"equal","card":"BP05-057"},
        {"id":"greater","card":"BP03-055"},
        {"id":"tail","card":"BP18-005"}
    ]);
    let mut game = game(&initial);
    let play = enter_with_fanfare(&mut game, "BP03-055");
    let play = choose_card_by_number(&mut game, play, "BP03-056");
    drop(accept(&mut game, play));
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["field"], json!(["source", "greater"]));
    assert_eq!(after["P1"]["cemetery"], json!(["material"]));
    assert_eq!(after["P1"]["deck"], json!(["tail", "equal"]));
}
