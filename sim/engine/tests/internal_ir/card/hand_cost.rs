use super::*;
use sve_engine::game::internal::InputDomain;
use sve_engine::ir::lowering::ExpandedLowerer;

fn program(later_failure: bool) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut ability = tree["cards"]["BP08-038"]["abilities"][0].clone();
    ability.as_object_mut().unwrap().remove("fanfare");
    let mut costs = vec![
        json!({"pp":1}),
        json!({"act_self":true}),
        json!({"discard":{"sel":{"who":"self","zone":"hand","kind":"spell"},"n":1}}),
    ];
    if later_failure {
        costs.push(json!({"pp":99}));
    }
    ability["activate"] = json!({"cost":{"steps":costs},"do":[{"draw":{"n":1}}]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[ability]}});
    ExpandedLowerer::new()
        .unwrap()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn hand_cost_selection_is_private_strict_and_payment_rolls_back_as_a_whole() {
    for (has_spell, acted, later_failure) in [
        (true, false, false),
        (false, false, false),
        (true, true, false),
        (true, false, true),
    ] {
        let mut initial = setup(1, json!([{"id":"d","card":"unit-spell"}]));
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"s","card":"unit-follower","state":{"acted":acted}}]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"h","card":if has_spell {"unit-spell"} else {"unit-follower"}}, {"id":"other","card":"unit-follower"}]);
        let mut game =
            Game::new(catalog(), &initial, &Value::Null, &Value::Null, "hand-cost").unwrap();
        game.load_internal_card("unit-follower", &program(later_failure))
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let before = game.projection(View::Referee).unwrap();
        let start = game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 });
        if !has_spell {
            start.unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            continue;
        }
        let CardPlay::Awaiting(question) = start.unwrap() else {
            panic!("hand materials");
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("cost materials");
        };
        assert_eq!(domain.members.len(), 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            2
        );
        assert!(game.internal_decision(View::P2).is_none());
        let pending = game.projection(View::Referee).unwrap();
        for handles in [vec![], vec![0, 0], vec![99]] {
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(handles),
            )
            .unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), pending);
        }
        let result = game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0]),
        );
        if acted || later_failure {
            result.unwrap_err();
            assert_eq!(game.projection(View::Referee).unwrap(), before);
            assert!(game.internal_decision(View::Referee).is_none());
        } else {
            let result = resolved(result.unwrap());
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["cemetery"], json!(["h"]));
            assert_eq!(view["P1"]["hand"], json!(["other", "d"]));
            assert_eq!(view["P1"]["pp"]["current"], 1);
            assert_eq!(view["objects"]["s"]["acted"], true);
            let discard = result
                .progress
                .facts
                .iter()
                .position(|fact| matches!(fact, Fact::Discard(_)))
                .unwrap();
            let played = result
                .progress
                .facts
                .iter()
                .position(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                .unwrap();
            assert!(discard < played);
        }
    }
}
