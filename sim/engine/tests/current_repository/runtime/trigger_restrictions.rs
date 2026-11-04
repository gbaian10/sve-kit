use super::*;
const BASE: [&str; 6] = [
    "ECP01-010",
    "ECP01-SL05",
    "ECP01-SP05",
    "ECP01-SSP05",
    "BP22-R18",
    "BP22-SL30",
];
const FORM: [&str; 6] = [
    "ECP01-011",
    "ECP01-SL06",
    "ECP01-SP06",
    "ECP01-SSP06",
    "BP22-R19",
    "BP22-SL31",
];

fn position(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "gentildonna-trigger-permission",
    )
    .unwrap()
}

#[test]
fn gentildonna_all_printings_suppress_enemy_fanfare_without_suppressing_generic_entry_observers() {
    // Every printing must exercise the live aura through one shared repository selection.
    for number in BASE.into_iter().chain(FORM) {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"banner","card":"BP01-033"},{"id":"incoming","card":"BP01-040"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"companion","card":"BP01-040"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"aura","card":number}]);
        let mut game = position(&initial);
        let original = game.projection(View::Referee).unwrap();
        let incoming_power = original["objects"]["incoming"]["power"].as_u64().unwrap();
        let companion_power = original["objects"]["companion"]["power"].as_u64().unwrap();
        assert!(matches!(
            game.play_internal_card("banner", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        meal_race::settle(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["companion"]["power"],
            companion_power
        );
        assert!(matches!(
            game.play_internal_card("incoming", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("the banner's ordinary entry observation must be pending");
        };
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].card, "BP01-033");
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert!(game.internal_pending_choice().unwrap().is_none());
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["objects"]["incoming"]["power"], incoming_power + 1);
        assert_eq!(after["objects"]["companion"]["power"], companion_power);
    }
}

#[test]
fn gentildonna_all_evolution_printings_damage_other_followers_unless_an_opponent_prohibits_the_occurrence()
 {
    for (base, form) in BASE.into_iter().zip(FORM) {
        for forbidden in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([
                {"id":"source","card":base},
                {"id":"friend","card":"BP01-040","state":{"hp":10_u8}}
            ]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":if forbidden {"BP22-R18"} else {"BP01-037"},"state":{"hp":10_u8}}]);
            let mut game = position(&initial);
            let result = game.internal_rule_action(&json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})).unwrap();
            assert_eq!(result.outcome, "resolved");
            meal_race::settle(&mut game);
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(
                after["objects"]["friend"]["hp"],
                if forbidden { 10_u8 } else { 3_u8 }
            );
            assert_eq!(
                after["objects"]["enemy"]["hp"],
                if forbidden { 10_u8 } else { 3_u8 }
            );
            assert!(result.events.iter().any(|event| event["kind"] == "進化"
                && event["object"] == "source"
                && event["with"] == "form"));
            assert_eq!(after["objects"]["source"]["hp"], 7_u8);
            assert_eq!(after["P1"]["pp"]["current"], 9_u8);
        }
    }
}

#[test]
fn gentildonna_base_printings_pay_the_actual_meal_and_race_while_their_aura_is_live() {
    for number in BASE {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":number}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot","card":"CSD01-032"}]);
        let mut game = position(&initial);
        meal_race::feed(&mut game, number, 0, false);
        meal_race::settle(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["race"], json!(["carrot"]));
        assert_eq!(after["P1"]["pp"]["current"], 9_u8);
    }
}
