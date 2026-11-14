use super::*;
use alloc::collections::BTreeMap;

#[test]
fn a_public_destroy_query_uses_its_current_posture_filter() {
    let tree = equipment_played::tree(
        &json!({"spell":{"do":[{"destroy":{"ref":{"who":"opp","zone":"field","kind":"follower","acted":true}}}]}}),
        "unit-spell",
    );
    let program = lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"acted","card":"unit-follower","state":{"acted":true}},
        {"id":"standing","card":"unit-follower"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "public-destroy-filter",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["standing"]));
    assert_eq!(state["P2"]["cemetery"], json!(["acted"]));
}
