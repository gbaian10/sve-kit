#![expect(
    clippy::unreachable,
    reason = "Unexpected authority decisions invalidate these synthetic scenarios."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

#[test]
fn source_counter_sets_the_public_target_limit() {
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"allies","from":{"who":"self","zone":"field","kind":"follower","other":true},"upto":{"counter":{"on":"this","name":"charge"}}}],"do":[{"buff":{"on":"allies","power":1,"hp":1}}]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for count in [0_u8, 1, 2] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower","state":{"counters":{"charge":count}}},{"id":"ally-a","card":"unit-follower"},{"id":"ally-b","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "source-counter-quantity",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        let step = game
            .activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap();
        if let CardPlay::Awaiting(q) = step {
            let InputDomain::Targets(domain) = &q.domain else {
                unreachable!()
            };
            assert_eq!(domain.cardinality.maximum, Some(Nat(u64::from(count))));
            let handles = domain
                .members
                .iter()
                .take(usize::from(count))
                .map(|m| m.handle)
                .collect();
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
                    .unwrap(),
            );
        }
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["power"], 2);
        let buffed = ["ally-a", "ally-b"]
            .iter()
            .filter(|id| state["objects"][**id]["power"] == 3)
            .count();
        assert_eq!(buffed, usize::from(count));
    }
}

#[test]
fn source_power_sets_the_public_original_cost_limit() {
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"ally","from":{"who":"self","zone":"field","kind":"follower","other":true,"orig_cost_le":{"power_of":"this"}},"n":1}],"do":[{"buff":{"on":"ally","power":1,"hp":1}}]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for power in [2_u8, 4] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower","state":{"power":power}},{"id":"ally","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = Game::new(
            catalog_with_cost(&json!("3")),
            &initial,
            &Value::Null,
            &Value::Null,
            "source-power-limit",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        let before = exact_authority(&game);
        let step = game.activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        );
        if power < 3 {
            assert!(step.is_err() || matches!(step, Ok(CardPlay::Cancelled)));
            assert_eq!(exact_authority(&game), before);
        } else {
            let CardPlay::Awaiting(q) = step.unwrap() else {
                unreachable!()
            };
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
                    .unwrap(),
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["ally"]["power"],
                3
            );
        }
    }
}
