use super::*;
use sve_engine::game::internal::ResumeStage;

#[test]
fn installed_cost_waits_for_the_next_actual_target_turn_then_expires() {
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"do":[
            {"player_effect":{"who":"opp","until":{"during_next_turn":{"who":"opp"}},
                "effect":{"cost_mod":{"when_playing":{"who":"opp"},"delta":1}}}}
        ]}}),
        "unit-amulet",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["semantic_state"]["turn_schedule"] = json!({"extra":[{"player":"P1","count":1}]});
    for seat in ["P1", "P2"] {
        initial["players"][seat]["pp"] = json!({"current":10,"max":10});
        initial["players"][seat]["zones"]["field"] = json!([]);
        initial["players"][seat]["zones"]["hand"] = json!([]);
        initial["players"][seat]["zones"]["deck"] = json!(
            (0..6)
                .map(|i| json!({"id":format!("{seat}-d{i}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([
        {"id":"taxed","card":"unit-spell"},{"id":"later","card":"unit-spell"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "next-turn-cost",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![play::spell(vec![])]))
        .unwrap();
    resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    play::next_turn(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["active"],
        "P1"
    );
    play::next_turn(&mut game);
    let before = game.projection(View::Referee).unwrap()["P2"]["pp"]["current"]
        .as_i64()
        .unwrap();
    resolved(
        game.play_internal_card("taxed", PlayerId { seat: 2 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["pp"]["current"],
        before - 2
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    play::next_turn(&mut game);
    play::next_turn(&mut game);
    let before = game.projection(View::Referee).unwrap()["P2"]["pp"]["current"]
        .as_i64()
        .unwrap();
    resolved(
        game.play_internal_card("later", PlayerId { seat: 2 })
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["pp"]["current"],
        before - 1
    );
}
