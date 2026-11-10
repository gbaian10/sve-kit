// The shared repository selection makes the first printed scenario pay the source preparation cost.
#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the guardian's printed timing."
)]
use super::event_cemetery::finish_checks;
use super::*;

fn trigger(board: &mut Game, number: &str, line: u64) -> CardPlay {
    finish_checks(board);
    let question = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed pending ability")
    };
    assert_eq!(
        question.actor.seat,
        if number == "BP05-092" { 2_u32 } else { 1_u32 },
        "the active player resolves victory before the defending guardian"
    );
    let handle = options
        .iter()
        .find(|o| o.card == number && o.line == line)
        .unwrap()
        .handle;
    board
        .answer_internal_card(question.actor, question.token, Answer::Pending(handle))
        .unwrap()
}
#[test]
fn bp05_092_blocks_lethal_attack_and_active_players_victory_then_restores_one_life() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"attacker","card":"BP04-003","state":{"entered_this_turn":false}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"guardian","card":"BP05-092"}]);
    initial["players"]["P2"]["leader"]["life"] = json!(2_u8);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "guardian-Q775",
    )
    .unwrap();
    game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"P2.leader"}))
        .unwrap();
    finish_checks(&mut game);
    let quick = game.internal_quick_choice().unwrap().unwrap();
    game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
        .unwrap();
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], -6_i64);
    assert_ne!(state["game"]["ended"], true);
    resolved(trigger(&mut game, "BP04-003", 2));
    finish_checks(&mut game);
    assert_ne!(
        game.projection(View::Referee).unwrap()["game"]["ended"],
        true
    );
    let restored = resolved(trigger(&mut game, "BP05-092", 2));
    assert!(restored.progress.facts.iter().any(|fact| matches!(fact, Fact::LifeChange(event) if event.before == Int(-6) && event.after == Int(1) && event.delta == Int(7))));
    finish_checks(&mut game);
    let restored_state = game.projection(View::Referee).unwrap();
    assert_eq!(restored_state["P2"]["leader"]["life"], 1_i64);
    assert_eq!(restored_state["P2"]["field"], json!([]));
    assert_eq!(restored_state["P2"]["cemetery"], json!(["guardian"]));
    assert_ne!(restored_state["game"]["ended"], true);
    assert!(game.internal_pending_choice().unwrap().is_none());
}
