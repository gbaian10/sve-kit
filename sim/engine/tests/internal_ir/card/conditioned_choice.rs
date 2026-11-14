use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

fn tree() -> Value {
    evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "cost":{"act_self":true},
            "targets":[
                {"bind":"ally","option":1,"from":{"who":"self","zone":"field","kind":"follower","other":true},"n":1},
                {"bind":"enemy","option":2,"from":{"who":"opp","zone":"field","kind":"follower"},"n":1}
            ],
            "do":[{"if":{"cond":{"combo":3},"then":[{"choice":{"n":1,"options":[
                [{"buff":{"on":"ally","power":2,"hp":2}}],
                [{"buff":{"on":"enemy","power":-2,"hp":-2}}]
            ]}}]}}]
        }),
    )])
}

fn program() -> WellFormedCardProgram {
    lowerer()
        .lower(&tree())
        .unwrap()
        .remove("unit-follower")
        .unwrap()
}

#[test]
fn conditioned_choice_skips_modes_and_targets_below_combo_but_always_pays_source_cost() {
    let program = program();
    for seat in [1_u32, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        for combo in [0_u8, 2, 3] {
            for mode in [ModeId(1), ModeId(2)] {
                let mut initial = setup(seat, json!([]));
                initial["turn"]["active"] = json!(actor);
                initial["players"][actor]["zones"]["hand"] = json!([]);
                initial["players"][actor]["zones"]["field"] = json!([
                    {"id":"source","card":"unit-follower"},
                    {"id":"ally","card":"unit-follower"}
                ]);
                initial["players"][opponent]["zones"]["field"] =
                    json!([{"id":"enemy","card":"unit-follower"}]);
                initial["semantic_state"]["counters_this_turn"][actor]["cards_played"] =
                    json!(combo);
                let mut game = evolution_linked_movement::loaded(&initial, &program);
                let mut play = game
                    .activate_internal_card(
                        "source",
                        program.card().abilities[0].node.id,
                        PlayerId { seat },
                    )
                    .unwrap();
                if combo >= 3 {
                    let CardPlay::Awaiting(question) = play else {
                        panic!("conditioned modes");
                    };
                    let InputDomain::Modes { modes, .. } = &question.domain else {
                        panic!("mode declaration");
                    };
                    assert_eq!(modes, &[ModeId(1), ModeId(2)]);
                    let CardPlay::Awaiting(target) = game
                        .answer_internal_card(
                            question.actor,
                            question.token,
                            Answer::Modes(vec![mode]),
                        )
                        .unwrap()
                    else {
                        panic!("selected mode target");
                    };
                    let InputDomain::Targets(domain) = &target.domain else {
                        panic!("one formal target");
                    };
                    assert_eq!(domain.members.len(), 1);
                    assert_eq!(
                        game.projection(View::Referee).unwrap()["objects"]["source"]["acted"],
                        false
                    );
                    play = game
                        .answer_internal_card(
                            target.actor,
                            target.token,
                            Answer::Targets(vec![domain.members[0].handle]),
                        )
                        .unwrap();
                }
                resolved(play);
                let view = game.projection(View::Referee).unwrap();
                assert_eq!(view["objects"]["source"]["acted"], true);
                let allied = combo >= 3 && mode == ModeId(1);
                let enemy = combo >= 3 && mode == ModeId(2);
                assert_eq!(view["objects"]["ally"]["power"], if allied { 4 } else { 2 });
                assert_eq!(view["objects"]["ally"]["hp"], if allied { 5 } else { 3 });
                assert_eq!(view["objects"]["enemy"]["power"], if enemy { 0 } else { 2 });
                assert_eq!(view["objects"]["enemy"]["hp"], if enemy { 1 } else { 3 });
            }
        }
    }
}

#[test]
fn conditioned_choice_quick_quote_skips_inactive_mode_targets_and_checks_active_ones() {
    let mut tree = tree();
    let mut body = tree["cards"]["unit-follower"]["abilities"][0]["activate"].take();
    body.as_object_mut().unwrap().remove("cost");
    body["targets"].as_array_mut().unwrap().remove(0);
    body["targets"][0]["option"] = json!(1);
    body["do"][0]["if"]["then"][0]["choice"]["options"] = json!([
        [{"damage":{"to":"enemy","n":1}}]
    ]);
    let mut source = tree["cards"]["unit-follower"].take();
    source["abilities"][0]
        .as_object_mut()
        .unwrap()
        .remove("activate");
    source["abilities"][0]["spell"] = body;
    tree["cards"] = json!({"unit-spell":source});
    let mut lowered = lowerer()
        .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".into(), Nat(1))]))
        .unwrap();
    let spell = lowered.remove("unit-spell").unwrap().card().abilities[0].clone();
    let program = card(vec![spell, quick::permit()]);
    for (combo, enemy, offered) in [
        (0_u8, false, true),
        (2, false, false),
        (2, true, true),
        (3, false, false),
        (3, true, true),
    ] {
        let mut initial = setup(1, json!([]));
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"unit-spell"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        initial["players"]["P2"]["zones"]["field"] = if enemy {
            json!([{"id":"enemy","card":"unit-follower"}])
        } else {
            json!([])
        };
        initial["semantic_state"]["counters_this_turn"]["P1"]["cards_played"] = json!(combo);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "conditional-quick",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let question = game.internal_quick_choice().unwrap().unwrap();
        let InputDomain::Quick { options, .. } = &question.domain else {
            panic!("Quick options");
        };
        assert_eq!(options.len(), usize::from(offered));
    }
}
