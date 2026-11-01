use super::*;

fn program(set: bool) -> WellFormedCardProgram {
    program_with(set, false, false)
}

fn program_with(set: bool, all: bool, permanent: bool) -> WellFormedCardProgram {
    let mut tree = fanfare::expanded::tree().clone();
    let mut base = tree["cards"]["synthetic-evolution"].clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    let change = if set {
        json!({"on":"affected","set":0})
    } else {
        json!({"on":"affected","delta":-1_i8})
    };
    entry["activate"] = json!({"cost":{"pp":0},"do":[{"apply":{"on":"this","effect":{"evolve_cost_mod":change},"until":"turn"}}]});
    if all {
        entry["activate"]["do"][0]["apply"]["on"] =
            json!({"who":"self","zone":"field","kind":"follower"});
    }
    if permanent {
        entry["activate"]["do"][0]["apply"]
            .as_object_mut()
            .unwrap()
            .remove("until");
    }
    base["abilities"].as_array_mut().unwrap().push(entry);
    tree["cards"] = json!({"unit-follower":base});
    lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn source_evolution_delta_and_set_change_only_its_actual_rule_payment() {
    for set in [false, true] {
        let program = program(set);
        let definition = program
            .card()
            .abilities
            .iter()
            .find(|ability| matches!(ability.node.source.origin, AbilityOrigin::Printed { .. }))
            .unwrap()
            .node
            .id;
        let mut game = evolution::position(1, &program, &card(vec![]));
        resolved(
            game.activate_internal_card("s", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        for (holder, pp) in [("s", 0_u8), ("u", 1)] {
            let mut attempt = game.clone();
            let before = attempt.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
                .as_u64()
                .unwrap();
            assert_eq!(attempt.internal_rule_action(&json!({"do":"evolve","source":holder,"evolve_card":"e","pay":{"pp":pp,"ep":0,"sep":0}})).unwrap().outcome, "resolved");
            let state = attempt.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], before - u64::from(pp));
            assert_eq!(state["objects"][holder]["evolved_with"], "e");
        }
    }
}

#[test]
fn repeated_evolution_reductions_floor_the_final_payment_without_refunding_pp() {
    let program = program(false);
    let definition = program
        .card()
        .abilities
        .iter()
        .find(|ability| matches!(ability.node.source.origin, AbilityOrigin::Printed { .. }))
        .unwrap()
        .node
        .id;
    let mut game = evolution::position(1, &program, &card(vec![]));
    for _ in 0..2_u8 {
        resolved(
            game.activate_internal_card("s", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
    }
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":0,"ep":0,"sep":0}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        2_u8
    );
}

#[test]
fn evolution_modifier_rejects_ambiguous_or_negative_set_values() {
    for change in [
        json!({"on":"affected","set":-1_i8}),
        json!({"on":"affected","set":0,"delta":-1_i8}),
    ] {
        let mut tree = fanfare::expanded::tree().clone();
        tree["cards"]["BP08-038"]["abilities"][0]["fanfare"]["do"] =
            json!([{"apply":{"on":"this","effect":{"evolve_cost_mod":change},"until":"turn"}}]);
        lowerer().lower(&tree).unwrap_err();
    }
}

#[test]
fn public_field_evolution_modifier_changes_each_selected_followers_payment() {
    let program = program_with(true, true, false);
    let definition = program
        .card()
        .abilities
        .iter()
        .find(|ability| matches!(ability.node.source.origin, AbilityOrigin::Printed { .. }))
        .unwrap()
        .node
        .id;
    let mut game = evolution::position(1, &program, &card(vec![]));
    resolved(
        game.activate_internal_card("s", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    for source in ["s", "u"] {
        let mut attempt = game.clone();
        assert_eq!(
            attempt
                .internal_rule_action(
                    &json!({"do":"evolve","source":source,"evolve_card":"e","pay":{"pp":0}})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        assert_eq!(
            attempt.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            2
        );
    }
}

#[test]
fn turn_scoped_evolution_modifiers_expire_while_unlimited_modifiers_survive() {
    for permanent in [false, true] {
        let program = program_with(true, false, permanent);
        let definition = program
            .card()
            .abilities
            .iter()
            .find(|ability| matches!(ability.node.source.origin, AbilityOrigin::Printed { .. }))
            .unwrap()
            .node
            .id;
        let mut game = evolution::position_with_decks(1, &program, &card(vec![]), true);
        resolved(
            game.activate_internal_card("s", definition, PlayerId { seat: 1 })
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        for _ in 0..2 {
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            while let Some(question) = game.internal_quick_choice().unwrap() {
                game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                    .unwrap();
            }
            if game.projection(View::Referee).unwrap()["flow"]["kind"] == "end" {
                game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
                    .unwrap();
            }
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            while let Some(question) = game.internal_quick_choice().unwrap() {
                game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
                    .unwrap();
            }
        }
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["active"],
            "P1"
        );
        let payment = u8::from(!permanent);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":payment}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
    }
}
