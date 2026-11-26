use super::*;

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-171"},
            {"id":format!("{seat}-second"),"card":"BP01-171"}
        ]);
    }
    initial
}

fn finish(game: &mut Game, step: CardPlay) {
    finish_mode(game, step, ModeId(1));
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    checks(game);
}

fn trigger(game: &mut Game, card: &str) {
    trigger_mode(game, card, ModeId(1));
}

fn trigger_mode(game: &mut Game, card: &str, mode: ModeId) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("expected a pending ability")
    };
    let chosen = options.iter().find(|option| option.card == card).unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(chosen.handle))
        .unwrap();
    finish_mode(game, step, mode);
    checks(game);
}

#[test]
fn bp06_105_plays_off_turn_and_uses_four_damage_with_a_friendly_ward() {
    let mut initial = position("BP06-105");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ward","card":"BP01-046"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(q.actor, actor());
    let InputDomain::Quick { options, .. } = &q.domain else {
        panic!("Quick window")
    };
    let chosen = options.iter().find(|o| o.card == "BP06-105").unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(Some(chosen.handle)))
        .unwrap();
    finish(&mut game, step);
    checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn bp16_011_uses_a_real_follower_death_to_enable_play_and_four_damage() {
    let mut initial = position("BP16-011");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"victim","card":"BP01-034"}]);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"kill","card":"BP01-116"}));
    let mut game = game(&initial);
    play(&mut game, "kill");
    play(&mut game, "source");
    trigger(&mut game, "BP16-011");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["victim", "kill"]));
}
