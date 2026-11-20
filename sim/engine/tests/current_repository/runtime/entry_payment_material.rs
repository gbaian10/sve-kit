use super::*;

#[test]
fn phantom_blaster_dragon_evolves_from_its_committed_additional_material() {
    let mut initial = position_with_source("CP03-084", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fee","card":"CP03-086"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP03-085"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    let play = choose_card_by_number(&mut game, play, "CP03-086");
    assert!(matches!(accept(&mut game, play), CardPlay::Entered(_)));
    checks(&mut game);
    let play = resolve_pending_card(&mut game, "CP03-084");
    let play = choose_card_by_number(&mut game, play, "CP03-085");
    drop(accept(&mut game, play));
    checks(&mut game);

    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(after["objects"]["source"]["evolved_with"], "form");
    assert_eq!(after["P1"]["pp"]["current"], 7);
}
