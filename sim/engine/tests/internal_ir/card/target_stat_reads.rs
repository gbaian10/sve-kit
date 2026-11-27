use super::*;
use sve_engine::game::internal::InputDomain;

fn amulet_program(activate: &Value) -> WellFormedCardProgram {
    let tree = equipment_played::tree(&json!({ "activate": activate }), "unit-amulet");
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn board(program: &WellFormedCardProgram, cost: &str, zones: &Value) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":10_u8});
    initial["players"]["P1"]["zones"] = zones.clone();
    initial["players"]["P2"]["zones"] = json!({"field":[]});
    let mut game = Game::new(
        catalog_with_cost(&json!(cost)),
        &initial,
        &Value::Null,
        &Value::Null,
        "target-stat-reads",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

fn activate_with_first_targets(game: &mut Game, program: &WellFormedCardProgram) {
    let mut step = game
        .activate_internal_card(
            "p",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap();
    while let CardPlay::Awaiting(question) = &step {
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("only target questions are expected");
        };
        step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
    }
    resolved(step);
    game.advance_player_input();
}

#[test]
fn a_play_point_recovery_may_read_a_chosen_followers_original_cost() {
    let program = amulet_program(&json!({
        "cost":{"pp":0},
        "targets":[{"bind":"ally","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],
        "do":[{"pp":{"who":"self","delta":{"orig_cost_of":"ally"}}}]
    }));
    let mut game = board(
        &program,
        "3",
        &json!({"field":[{"id":"p","card":"unit-amulet"},{"id":"ally","card":"unit-follower"}]}),
    );
    activate_with_first_targets(&mut game, &program);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3_u8
    );
}

#[test]
fn a_chosen_cemetery_follower_lends_its_attack_before_it_returns() {
    let program = amulet_program(&json!({
        "cost":{"pp":0},
        "targets":[{"bind":"fallen","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}],
        "do":[
            {"damage":{"to":{"leaders":"opponents"},"n":{"power_of":"fallen"}}},
            {"move":{"ref":"fallen","to":"field","result":"entered"}}
        ]
    }));
    let mut game = board(
        &program,
        "1",
        &json!({
            "field":[{"id":"p","card":"unit-amulet"}],
            "cemetery":[{"id":"fallen","card":"unit-follower"}]
        }),
    );
    activate_with_first_targets(&mut game, &program);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["field"], json!(["p", "fallen"]));
}

#[test]
fn a_cemetery_target_lends_only_its_attack() {
    let tree = equipment_played::tree(
        &json!({"activate":{
            "cost":{"pp":0},
            "targets":[{"bind":"fallen","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}],
            "do":[{"damage":{"to":{"leaders":"opponents"},"n":{"orig_cost_of":"fallen"}}}]
        }}),
        "unit-amulet",
    );
    lowerer().lower(&tree).unwrap_err();
}
