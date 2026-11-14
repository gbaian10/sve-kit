use super::*;

#[test]
fn an_owned_hand_ability_moves_only_its_bound_source_to_ex_before_adding_a_counter() {
    let tree = equipment_played::tree(
        &json!({"activate":{"zone":"hand","cost":{"pp":0},"do":[{"move":{"ref":"this","to":"ex","result":"moved"}},{"counter":{"on":{"follow":"moved"},"name":"charge","delta":1}}]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"unit-follower"},
        {"id":"other","card":"unit-follower"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "hand-source-ex",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    resolved(
        game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["other"]));
    assert_eq!(state["P1"]["ex"], json!(["source"]));
    assert_eq!(state["objects"]["source"]["counters"]["charge"], 1);
}
