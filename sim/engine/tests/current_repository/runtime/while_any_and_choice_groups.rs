#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
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

fn keyword(state: &Value, object: &str, word: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(word))
}

fn quick_pass(game: &mut Game) {
    for _ in 0_u8..4 {
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("The Quick window must finish");
}

#[test]
fn bp08_113_takes_one_follower_and_one_amulet_from_seven_and_reveals_both() {
    let mut initial = position("BP08-113");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"follower","card":"BP01-034"},{"id":"amulet","card":"BP01-041"},{"id":"a","card":"BP01-116"},{"id":"b","card":"BP01-116"},{"id":"c","card":"BP01-116"},{"id":"d","card":"BP01-116"},{"id":"e","card":"BP01-116"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["follower", "amulet"]));
    assert_eq!(state["P1"]["deck"], json!(["e", "d", "c", "b", "a"]));
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|c| c["id"] == "follower"),
        "The chosen follower is revealed"
    );
    assert!(
        known.iter().any(|c| c["id"] == "amulet"),
        "The chosen amulet is revealed"
    );
    assert!(
        !known.iter().any(|c| c["id"] == "a"),
        "The unselected cards remain private"
    );
}

#[test]
fn ecp02_069_uses_a_risa_on_the_field_for_keywords_and_a_magic_item() {
    let mut initial = position("ECP02-069");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"risa","card":"CP02-093"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "ECP02-069");
    let state = game.projection(View::Referee).unwrap();
    assert!(keyword(&state, "source", "突進"));
    assert!(keyword(&state, "source", "指定攻撃"));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1_usize);
}

#[test]
fn bp14_062_damages_an_acted_enemy_at_end_phase_and_prevents_its_next_start_stand() {
    let mut initial = position("BP14-062");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP14-062");
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    trigger(&mut game, "BP14-062");
    quick_pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["objects"]["enemy"]["acted"], true);
}
