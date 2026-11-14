use super::*;

#[test]
fn body_source_posture_reads_the_current_field_holder_at_resolution() {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"if":{"cond":{"standing":"this"},"then":[{"life":{"who":"self","delta":2}}],"else":[{"life":{"who":"self","delta":1}}]}}]}),
    )]);
    let programs = lowerer().lower(&tree).unwrap();
    let program = &programs["unit-follower"];
    for acted in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower","state":{"acted":acted}}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "body-source-posture",
        )
        .unwrap();
        game.load_internal_card("unit-follower", program).unwrap();
        resolved(
            game.activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if acted { 21 } else { 22 }
        );
    }
}

#[test]
fn departed_source_has_no_current_field_posture() {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({"cost":{"pp":0},"do":[{"move":{"ref":"this","to":"cemetery"}},{"if":{"cond":{"standing":"this"},"then":[{"life":{"who":"self","delta":1}}],"else":[{"life":{"who":"self","delta":2}}]}}]}),
    )]);
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "departed-source-posture",
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
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22
    );
}
