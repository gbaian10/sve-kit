use super::*;

#[test]
fn paid_continuations_can_consume_the_same_direct_event_subject() {
    let catalog = catalog();
    let subject_move = json!({"move":{"ref":"it","to":"cemetery"}});
    let payment = |steps: Value| json!({"pay":{"cost":{"act_self":true},"do":steps}});
    for (step, admitted) in [
        (subject_move.clone(), true),
        (
            payment(json!([{"move":{"ref":"this","to":"cemetery"}}])),
            true,
        ),
        (payment(json!([subject_move])), true),
    ] {
        let tree = equipment_played::tree(
            &json!({"auto":{
                "on":{"enter_field":{"who":"self","sel":{"kind":"follower"}}},
                "do":[step]
            }}),
            "unit-follower",
        );
        let program = lowerer()
            .lower(&tree)
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let mut game = Game::new(
            Arc::clone(&catalog),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "event-subject-payment-admission",
        )
        .unwrap();
        assert_eq!(
            game.load_internal_card("unit-follower", &program).is_ok(),
            admitted
        );
    }
}
