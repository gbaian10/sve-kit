use super::*;

#[test]
fn bp17_005_evolves_to_006_then_replaces_destroyed_followers_before_lastwords() {
    let mut initial = setup("P1", "P1", [20, 20]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP17-005"},
        {"id":"mine","card":"BP17-T01","owner":"P2"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-006"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"outside","card":"BP17-T01"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"theirs-1","card":"BP17-T01","owner":"P1"},
        {"id":"theirs-2","card":"BP17-T01","owner":"P1"}
    ]);
    let mut game = game(&initial);
    let before = game.projection(View::Referee).unwrap();
    if let Ok(step) = game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
    ) {
        assert_ne!(
            step.outcome, "resolved",
            "less than the printed cost cannot evolve"
        );
    }
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":3_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("real evolve trigger");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP17-006");
    resolved(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(state["P2"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(state["P1"]["ex"], json!(["outside"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
    assert_eq!(
        state["P1"]["leader"]["life"], 20_u8,
        "replacement tokens arrive before Lastwords resolve"
    );
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    for side in ["P1", "P2"] {
        for id in state[side]["field"].as_array().unwrap() {
            let id = id.as_str().unwrap();
            if id != "source" {
                assert_eq!(state["objects"][id]["card"], "BP17-T01");
            }
        }
    }
    for id in ["mine", "theirs-1", "theirs-2"] {
        assert!(
            state["objects"].get(id).is_none(),
            "destroyed tokens are erased"
        );
    }
    game.resume_internal_card().unwrap();
    let mut resolved_lastwords = 0_u8;
    while let Some(lastword) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(offered) = &lastword.domain else {
            panic!("destroyed-token Lastwords");
        };
        assert!(offered.iter().all(|option| option.card == "BP17-T01"));
        resolved(
            game.answer_internal_card(
                lastword.actor,
                lastword.token,
                Answer::Pending(offered[0].handle),
            )
            .unwrap(),
        );
        resolved_lastwords += 1_u8;
        game.resume_internal_card().unwrap();
    }
    assert_eq!(resolved_lastwords, 3_u8);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["leader"]["life"], 19_u8);
    assert_eq!(after["P2"]["leader"]["life"], 18_u8);
}
