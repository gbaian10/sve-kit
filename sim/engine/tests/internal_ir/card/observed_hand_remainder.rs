#![expect(
    clippy::unreachable,
    reason = "Unexpected decisions invalidate the observed remainder scenario."
)]
use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

#[test]
fn observed_remainder_moves_to_hand_after_actual_field_arrivals() {
    let tree = equipment_played::tree(
        &json!({"spell":{"do":[{"reveal":{"top":{"who":"self","n":3},"bind":"seen"}},{"choose":{"bind":"picked","from":{"any_of":[{"in":"seen","kind":"follower"},{"in":"seen","kind":"amulet"}]},"upto":2,"may":true}},{"move":{"ref":"picked","to":"field","result":"entered"}},{"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"entered","view":"before"}}},"to":"hand"}}]}}),
        "unit-spell",
    );
    let program = lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    for free in [0_usize, 2] {
        let mut initial = setup(
            1,
            json!([{"id":"follower","card":"unit-follower"},{"id":"amulet","card":"unit-amulet"},{"id":"rest","card":"unit-spell"}]),
        );
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..5 - free)
                .map(|i| json!({"id":format!("old-{i}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "observed-hand-remainder",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        let step = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        if let CardPlay::Awaiting(q) = step {
            let InputDomain::Targets(domain) = &q.domain else {
                unreachable!()
            };
            let limit = usize::try_from(domain.cardinality.maximum.unwrap_or(Nat(2)).0).unwrap();
            let handles = domain
                .members
                .iter()
                .take(limit)
                .map(|m| m.handle)
                .collect();
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
                    .unwrap(),
            );
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 5_usize);
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3 - free);
        assert_eq!(state["P1"]["deck"], json!([]));
        assert!(
            state["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("rest"))
        );
    }
}
