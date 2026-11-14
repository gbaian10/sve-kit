use super::*;
use alloc::collections::BTreeMap;

#[test]
fn spell_body_uses_its_original_play_zone_after_drawing() {
    let tree = equipment_played::tree(
        &json!({"spell":{"do":[{"draw":{"n":1}},{"if":{"cond":{"played_from":{"ref":"this","from":"hand"}},"then":[{"move":{"ref":"this","to":"ex"}}]}}]}}),
        "unit-spell",
    );
    let program = lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
        .unwrap()
        .remove("unit-spell")
        .unwrap();
    for (origin, full, in_ex) in [
        ("hand", false, true),
        ("hand", true, false),
        ("ex", false, false),
    ] {
        let mut initial = setup(1, json!([]));
        for seat in ["P1", "P2"] {
            initial["players"][seat]["zones"]["field"] = json!([]);
        }
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = if full {
            json!(
                (0..5)
                    .map(|i| json!({"id":format!("extra-{i}"),"card":"unit-follower"}))
                    .collect::<Vec<_>>()
            )
        } else {
            json!([])
        };
        initial["players"]["P1"]["zones"][origin]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"source","card":"unit-spell"}));
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "spell-origin",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        resolved(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            in_ex
        );
        assert_eq!(
            state["P1"]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            !in_ex
        );
    }
}
