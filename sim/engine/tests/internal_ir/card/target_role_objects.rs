#![expect(
    clippy::unreachable,
    reason = "Unexpected authority decisions invalidate these synthetic scenarios."
)]
use super::*;
use sve_engine::game::internal::{InputDomain, TargetRoleAssignment};

#[test]
fn declared_target_roles_feed_existing_damage_and_posture_actions() {
    let tree = equipment_played::tree(
        &json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"enemies","from":{"who":"opp","zone":"field","kind":"follower"},"n":2,"roles":{"primary":{"if_nonempty":1},"rest":"remaining"}}],"do":[{"damage":{"to":{"role":{"of":"enemies","name":"primary"}},"n":1}},{"act":{"role":{"of":"enemies","name":"rest"}}}]}}),
        "unit-follower",
    );
    let program = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"first","card":"unit-follower"},{"id":"second","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "target-role-objects",
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
    let primary = domain
        .roles
        .iter()
        .find(|r| matches!(r.cardinality, RoleCardinality::IfNonempty { .. }))
        .unwrap()
        .role;
    let rest = domain
        .roles
        .iter()
        .find(|r| matches!(r.cardinality, RoleCardinality::Remainder {}))
        .unwrap()
        .role;
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::TargetsWithRoles {
                targets: vec![0, 1],
                roles: vec![
                    TargetRoleAssignment {
                        role: primary,
                        targets: vec![0],
                    },
                    TargetRoleAssignment {
                        role: rest,
                        targets: vec![1],
                    },
                ],
            },
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["first"]["damage"], 1);
    assert_eq!(state["objects"]["first"]["acted"], false);
    assert_eq!(state["objects"]["second"]["damage"], 0);
    assert_eq!(state["objects"]["second"]["acted"], true);
}
