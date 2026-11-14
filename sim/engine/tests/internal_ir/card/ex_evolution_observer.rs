use super::*;
use sve_engine::game::internal::{InputDomain, ResumeStage};

#[test]
fn ex_evolution_observer_uses_its_declared_zone_and_original_controller() {
    let mut input = equipment_played::tree(
        &json!({"line":1,"evolve":{"cost":{"pp":0}}}),
        "unit-follower",
    );
    let observer=equipment_played::tree(&json!({"line":2,"auto":{"zone":"ex", "on":{"evolved":{"who":"self","sel":{"kind":"follower"}}},
        "do":[{"life":{"who":"self","delta":1}}]
    }}),"unit-follower")["cards"]["unit-follower"]["abilities"][0].clone();
    input["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(observer);
    let base = lowerer()
        .lower(&input)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let zones = json!({"P1":{"ex":[{"id":"own-ex","card":"unit-follower","owner":"P2"}]},
        "P2":{"ex":[{"id":"foreign-ex","card":"unit-follower"}]}});
    let mut game = evolution::position_with_zones(1, &base, &card(vec![]), false, &zones);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":0}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("EX pending");
    };
    assert_eq!(
        options.len(),
        1,
        "field copies are inactive and the opposing EX copy watches its own controller"
    );
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(result.source.controller, PlayerId { seat: 1 });
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20
    );
}
