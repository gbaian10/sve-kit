use super::*;

#[test]
fn bp05_061_removes_ward_and_blocks_attacks_through_the_next_opposing_turn() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP05-060"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP05-061"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP05-060"}]);
    for player in ["P1", "P2"] {
        initial["players"][player]["zones"]["deck"] = json!([
            {"id":format!("{player}-draw1"),"card":"BP01-037"},
            {"id":format!("{player}-draw2"),"card":"BP01-037"}
        ]);
    }
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "turn-ability-loss",
    )
    .unwrap();
    assert!(has_granted_keyword(
        &game.projection(View::Referee).unwrap(),
        "enemy",
        "守護"
    ));
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    finish_checks(&mut game);
    resolve_grant_trigger(&mut game, 2, Some("BP05-060"));
    assert!(!has_granted_keyword(
        &game.projection(View::Referee).unwrap(),
        "enemy",
        "守護"
    ));

    next_turn(&mut game);
    finish_checks(&mut game);
    let opposing_turn = game.projection(View::Referee).unwrap();
    assert_eq!(opposing_turn["turn"]["active"], "P2");
    assert!(!has_granted_keyword(&opposing_turn, "enemy", "守護"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"P1.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );

    next_turn(&mut game);
    finish_checks(&mut game);
    assert!(has_granted_keyword(
        &game.projection(View::Referee).unwrap(),
        "enemy",
        "守護"
    ));
}
