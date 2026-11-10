use super::*;

fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["banish"] = json!([{"id":"replay","card":"BP01-043"}]);
    initial
}
fn loaded(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "banish-play",
    )
    .unwrap()
}
fn finish(game: &mut Game) {
    event_cemetery::finish_checks(game);
}
// The session shares real source preparation; each alias exercises the whole native play path.
#[test]
fn jewel_dress_up_banish_play_all_printings_require_permission_and_pay_normal_cost() {
    for number in ["BP18-P52", "BP18-T03"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"token","card":number}]);
        let mut game = loaded(&initial);
        let before = game.projection(View::Referee).unwrap();
        game.play_internal_card("replay", PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
        let token_play = game
            .play_internal_card("token", PlayerId { seat: 1 })
            .unwrap();
        resolved(token_play);
        finish(&mut game);
        let replay = game
            .play_internal_card("replay", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(replay, CardPlay::Entered(_)));
        finish(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 7_u8);
        assert_eq!(after["P1"]["field"], json!(["replay"]));
        assert_eq!(after["P1"]["banish"], json!([]));
    }
}
