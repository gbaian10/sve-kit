use super::*;
use alloc::collections::BTreeMap;
fn finish(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}
fn next_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if game.internal_quick_choice().unwrap().is_none() {
        game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
            .unwrap();
    }
    damage_prevent::pass_quick(game);
}
#[test]
fn bp05_086_whole_card_can_evolve_by_skipping_its_next_turn_instead_of_four_pp() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"]["current"] = json!(0_u8);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-086"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP05-087"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0..4_u8)
                .map(|i| json!({"id":format!("{seat}-draw-{i}"),"card":"BP01-009"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "native-skip-turn-evolution",
    )
    .unwrap();
    let definitions = selection_with_rules().current["BP05-086"]
        .program
        .card()
        .abilities
        .iter()
        .map(|ability| (ability.source.line, ability.node.id))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","definition":definitions[&1].0,"pay":{"pp":4_u8}})).unwrap().outcome,
        "cannot-evolve","the ordinary printed route still requires its four PP");
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"P1-draw-0","definition":definitions[&2].0,"pay":{"pp":0_u8}})).unwrap().outcome,
        "cannot-evolve","an invalid form must not schedule a skipped turn");
    assert_eq!(game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","definition":definitions[&2].0,"pay":{"pp":0_u8}})).unwrap().outcome,
        "resolved","the second printed route pays one skipped own turn");
    finish(&mut game);
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("evolve-time");
        };
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        finish(&mut game);
    }
    next_turn(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2",
        "the opponent's normal next turn begins"
    );
    next_turn(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P2",
        "the paid own turn is skipped"
    );
    next_turn(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1",
        "the payment skips exactly one own turn"
    );
}
