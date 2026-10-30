use super::event_cemetery::finish_checks;
use super::*;

#[test]
fn bp12_043_printings_evolve_from_the_real_base_and_draw_only_for_five_cemetery_wizard_followers() {
    for printing in ["BP12-043", "BP12-P08", "EBD02-011"] {
        for count in [4_usize, 5] {
            let mut value = draw_position();
            value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP12-042"}]);
            value["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":printing}]);
            value["players"]["P1"]["zones"]["cemetery"] = json!(
                (0..count)
                    .map(|i| json!({"id":format!("wizard-{i}"),"card":"BP12-042"}))
                    .chain([
                        json!({"id":"wizard-spell","card":"BP01-065"}),
                        json!({"id":"wrong-trait","card":"BP01-037"})
                    ])
                    .collect::<Vec<_>>()
            );
            value["players"]["P1"]["zones"]["ex"] =
                json!([{"id":"outside-wizard","card":"BP12-042"}]);
            value["players"]["P2"]["zones"]["cemetery"] =
                json!([{"id":"opposing-wizard","card":"BP12-042"}]);
            value["players"]["P1"]["zones"]["deck"] =
                json!([{"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"}]);
            value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
            let mut game = Game::from_selected_effects(
                selection_with_rules().clone(),
                &value,
                &Value::Null,
                &Value::Null,
                "block-bp12",
            )
            .unwrap();
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            finish_checks(&mut game);
            let occurrence = game.internal_pending_choice().unwrap().unwrap();
            let InputDomain::Pending(options) = &occurrence.domain else {
                panic!("native evolution occurrence")
            };
            assert_eq!(options.len(), 1);
            let CardPlay::Awaiting(target) = game
                .answer_internal_card(
                    occurrence.actor,
                    occurrence.token,
                    Answer::Pending(options[0].handle),
                )
                .unwrap()
            else {
                panic!("the printed target is required at either cemetery count")
            };
            let InputDomain::Targets(domain) = &target.domain else {
                panic!("formal enemy follower target")
            };
            assert_eq!(domain.members.len(), 1);
            resolved(
                game.answer_internal_card(
                    target.actor,
                    target.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after["objects"]["enemy"]["damage"], 2_u8);
            assert_eq!(after["P1"]["pp"]["current"], 9_u8);
            let draws = usize::from(count == 5);
            assert_eq!(
                after["P1"]["hand"].as_array().unwrap().len(),
                before["P1"]["hand"].as_array().unwrap().len() + draws
            );
            assert_eq!(
                after["P1"]["deck"].as_array().unwrap().len() + draws,
                before["P1"]["deck"].as_array().unwrap().len()
            );
        }
    }
}
