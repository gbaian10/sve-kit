#![expect(
    clippy::unreachable,
    reason = "Only a formal target question is valid in this scalar capture scenario."
)]
use super::*;
use sve_engine::game::internal::InputDomain;

#[test]
fn optional_selected_original_cost_is_zero_for_empty_and_reads_one_target() {
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"chosen","from":{"who":"self","zone":"ex"},"upto":1}],"do":[{"damage":{"to":{"leader":"self"},"n":{"at":"selection","selection":"chosen","value":{"orig_cost_of":"chosen"}}}}]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for chosen in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["ex"] =
            json!([{"id":"candidate","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog_with_cost(&json!("3")),
            &initial,
            &Value::Null,
            &Value::Null,
            "optional-selected-cost",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &program).unwrap();
        let CardPlay::Awaiting(q) = game
            .activate_internal_card(
                "source",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            unreachable!()
        };
        let InputDomain::Targets(domain) = &q.domain else {
            unreachable!()
        };
        let handles = domain
            .members
            .iter()
            .take(usize::from(chosen))
            .map(|m| m.handle)
            .collect();
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if chosen { 17 } else { 20 }
        );
    }
}
