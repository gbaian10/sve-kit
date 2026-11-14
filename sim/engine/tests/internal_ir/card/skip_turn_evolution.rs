use super::*;

fn program() -> WellFormedCardProgram {
    let tree = equipment_played::tree(
        &json!({"evolve":{"cost":{"skip_turn":{"who":"self"}}}}),
        "unit-follower",
    );
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}
#[test]
fn skipped_turn_is_a_paid_evolution_cost_and_a_failed_form_does_not_schedule_it() {
    for valid in [false, true] {
        let base = program();
        let mut game = evolution::position(1, &base, &card(vec![]));
        if valid {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":0}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
        } else {
            let before = game.projection(View::Referee).unwrap();
            let outcome = game
                .internal_rule_action(
                    &json!({"do":"evolve","source":"s","evolve_card":"u","pay":{"pp":0}}),
                )
                .unwrap();
            assert_eq!(outcome.outcome, "cannot-evolve");
            assert_eq!(outcome.events, Vec::<Value>::new());
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        }
    }
}
