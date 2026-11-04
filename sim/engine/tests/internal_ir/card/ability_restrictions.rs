use super::*;
use sve_engine::game::internal::InputDomain;

fn restriction(except_evolution: bool) -> Value {
    let mut predicate = json!({"kind":"activated"});
    if except_evolution {
        predicate["except"] = json!(["evolution"]);
    }
    json!({"apply":{"on":"this","effect":{"cannot":{"what":"play_ability","ability":predicate}},"until":"turn"}})
}

fn programs(except_evolution: bool) -> (WellFormedCardProgram, WellFormedCardProgram) {
    let lock = restriction(except_evolution);
    let mut tree = evolution_linked_movement::tree(&[
        ("evolve", json!({"cost":{"pp":0}})),
        (
            "activate",
            json!({"quick":true,"cost":{"pp":0},"do":[lock]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":1}}]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"targets":[{"bind":"victim","from":{"who":"self","zone":"field","kind":"follower"},"n":1}],"do":[{"move":{"ref":"victim","to":"cemetery"}}]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"targets":[{"bind":"victim","from":{"who":"self","zone":"cemetery","kind":"follower"},"n":1}],"do":[{"move":{"ref":"victim","to":"field","result":"returned"}}]}),
        ),
        (
            "activate",
            json!({"quick":true,"cost":{"pp":0},"do":[{"life":{"who":"self","delta":1}}]}),
        ),
    ]);
    let mut form = tree["cards"]["unit-follower"].clone();
    form["abilities"].as_array_mut().unwrap().remove(0);
    tree["cards"]["unit-form"] = form;
    let lowered = lowerer().lower(&tree).unwrap();
    (
        lowered["unit-follower"].clone(),
        lowered["unit-form"].clone(),
    )
}

#[test]
fn activation_prohibition_follows_evolution_and_leaves_other_copies_independent() {
    let (base, form) = programs(true);
    let lock = base.card().abilities[1].node.id;
    let other = base.card().abilities[2].node.id;
    let evolved = form.card().abilities[1].node.id;
    for seat in [1, 2] {
        let actor = PlayerId { seat };
        let mut game = evolution::position_with_decks(seat, &base, &form, true);
        resolved(game.activate_internal_card("s", lock, actor).unwrap());
        game.resume_internal_card().unwrap();
        let before = exact_authority(&game);
        game.activate_internal_card("s", lock, actor).unwrap_err();
        game.activate_internal_card("s", other, actor).unwrap_err();
        assert_eq!(exact_authority(&game), before);
        resolved(game.activate_internal_card("u", other, actor).unwrap());
        game.resume_internal_card().unwrap();
        let step = game
            .internal_rule_action(
                &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":0}}),
            )
            .unwrap();
        assert_eq!(step.outcome, "resolved");
        let before = exact_authority(&game);
        game.activate_internal_card("s", evolved, actor)
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        for _ in 0..2 {
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap();
            for _ in 0..2 {
                if game.internal_quick_choice().unwrap().is_some() {
                    quick::pass(&mut game);
                }
            }
            assert!(game.internal_quick_choice().unwrap().is_none());
        }
        resolved(game.activate_internal_card("s", evolved, actor).unwrap());
    }
}

#[test]
fn an_unqualified_activation_prohibition_also_refuses_intrinsic_evolution_atomically() {
    let (base, form) = programs(false);
    let mut game = evolution::position(1, &base, &form);
    resolved(
        game.activate_internal_card("s", base.card().abilities[1].node.id, PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let before = game.projection(View::Referee).unwrap();
    let step = game
        .internal_rule_action(&json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":0}}))
        .unwrap();
    assert_eq!(step.outcome, "cannot-evolve");
    assert_eq!(step.events, Vec::<Value>::new());
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn activation_prohibition_keeps_other_subjects_and_lifetimes_closed() {
    for (on, until) in [("this", "next_opp_turn"), ("target", "turn")] {
        let mut lock = restriction(true);
        lock["apply"]["on"] = json!(on);
        lock["apply"]["until"] = json!(until);
        let tree =
            evolution_linked_movement::tree(&[("activate", json!({"cost":{"pp":0},"do":[lock]}))]);
        lowerer().lower(&tree).unwrap_err();
    }
}

#[test]
fn an_ordinary_departure_and_return_starts_a_new_unrestricted_generation() {
    let (base, form) = programs(true);
    let actor = PlayerId { seat: 1 };
    let mut game = evolution::position(1, &base, &form);
    resolved(
        game.activate_internal_card("s", base.card().abilities[1].node.id, actor)
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    for index in [3, 4] {
        let CardPlay::Awaiting(q) = game
            .activate_internal_card("u", base.card().abilities[index].node.id, actor)
            .unwrap()
        else {
            panic!("movement target")
        };
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
    }
    resolved(
        game.activate_internal_card("s", base.card().abilities[2].node.id, actor)
            .unwrap(),
    );
}

#[test]
fn raw_activation_permissions_cannot_install_a_wider_predicate_or_allow_effect() {
    let (base, form) = programs(true);
    for variant in 0..2 {
        let mut candidate = base.card().clone();
        let Node::Instruction { instruction } =
            &mut candidate.abilities[1].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("permission instruction")
        };
        let Action::InstallEffect { effect, .. } = &mut instruction.action else {
            panic!("installation")
        };
        let Effect::Permission {
            polarity,
            action: ActionPattern::PlayAbility { predicate },
            ..
        } = effect.as_mut()
        else {
            panic!("permission")
        };
        if variant == 0 {
            *polarity = Polarity::Allow;
        } else {
            predicate.kind = Some(AbilityKind::Triggered);
        }
        if let Ok(checked) = WellFormedCardProgram::check(candidate) {
            let mut game = evolution::position(1, &base, &form);
            let before = exact_authority(&game);
            game.load_internal_card("unit-follower", &checked)
                .unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
    }
}

#[test]
fn quick_preflight_omits_the_forbidden_copy_and_commits_the_other_copy() {
    let (base, form) = programs(true);
    let actor = PlayerId { seat: 1 };
    let mut game = evolution::position_with_decks(1, &base, &form, true);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(&mut game);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    assert_eq!(q.actor, actor);
    let InputDomain::Quick { options, .. } = &q.domain else {
        panic!("Quick options")
    };
    let lock = options
        .iter()
        .find(|option| option.position == 1 && option.line == Some(2))
        .unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(lock.handle)))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let q = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &q.domain else {
        panic!("Quick options")
    };
    assert_ne!(options.as_slice(), []);
    assert!(options.iter().all(|option| option.position == 2));
    let other = options
        .iter()
        .find(|option| option.line == Some(6))
        .unwrap();
    let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
        .as_u64()
        .unwrap();
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Quick(Some(other.handle)))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        before + 1
    );
}

#[test]
fn effect_authorization_does_not_bypass_an_activation_prohibition() {
    for forbidden in [false, true] {
        let mut tree = evolution_linked_movement::tree(&[
            (
                "activate",
                json!({"cost":{"pp":0},"do":[restriction(true)]}),
            ),
            (
                "activate",
                json!({"ub":true,"cost":{"steps":[{"pp":9},{"act_self":true}]},"do":[{"life":{"who":"self","delta":2}}]}),
            ),
        ]);
        let child = tree["cards"]["unit-follower"].clone();
        let mut parent = child.clone();
        let mut ability = parent["abilities"][0].clone();
        ability["activate"] = json!({"cost":{"pp":1},"targets":[{"bind":"friend","from":{"who":"self","zone":"field","kind":"follower","other":true},"n":1}],"do":[{"life":{"who":"self","delta":1}},{"play_ability":{"of":"friend","ability":{"ub":true,"n":1},"cost_policy":"waive_original"}},{"life":{"who":"self","delta":10}}]});
        parent["abilities"] = json!([ability]);
        tree["cards"] = json!({"unit-follower":child,"unit-other-follower":parent});
        let programs = lowerer().lower(&tree).unwrap();
        let actor = PlayerId { seat: 1 };
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["construction"] = json!("title");
        initial["players"]["P1"]["title_code"] = json!("ub_test");
        initial["players"]["P1"]["pp"] = json!({"current":10_u8,"max":10_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"parent","card":"unit-other-follower"},
            {"id":"child","card":"unit-follower"}
        ]);
        let mut game = Game::new(
            event_ability::title_catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "forbidden-effect-activation",
        )
        .unwrap();
        for (number, program) in &programs {
            game.load_internal_card(number, program).unwrap();
        }
        if forbidden {
            resolved(
                game.activate_internal_card(
                    "child",
                    programs["unit-follower"].card().abilities[0].node.id,
                    actor,
                )
                .unwrap(),
            );
            event_ability::finish(&mut game);
        }
        let CardPlay::Awaiting(target) = game
            .activate_internal_card(
                "parent",
                programs["unit-other-follower"].card().abilities[0].node.id,
                actor,
            )
            .unwrap()
        else {
            panic!("formal ability holder");
        };
        let InputDomain::Targets(domain) = &target.domain else {
            panic!("formal ability holder");
        };
        assert_eq!(domain.members.len(), 1);
        let play = game
            .answer_internal_card(
                target.actor,
                target.token,
                Answer::Targets(vec![domain.members[0].handle]),
            )
            .unwrap();
        let result = resolve_effect_activation(&mut game, play, forbidden);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 9_u8);
        assert_eq!(
            state["P1"]["leader"]["life"],
            if forbidden { 31_u8 } else { 33_u8 }
        );
        assert_eq!(state["objects"]["child"]["acted"], false);
        assert_eq!(
            state["semantic_state"]["counters_this_turn"]["P1"]["ub_activated"],
            u8::from(!forbidden),
        );
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
                .count(),
            if forbidden { 1 } else { 2 },
        );
    }
}

#[expect(
    clippy::panic,
    reason = "Unexpected formal ability decisions invalidate the scenario."
)]
fn resolve_effect_activation(game: &mut Game, play: CardPlay, forbidden: bool) -> Activation {
    if forbidden {
        return resolved(play);
    }
    let CardPlay::Awaiting(choice) = play else {
        panic!("effective UB choice");
    };
    let InputDomain::Abilities(options) = &choice.domain else {
        panic!("effective UB choice");
    };
    assert_eq!(
        options.len(),
        1,
        "only the unlocked child UB must be selectable"
    );
    resolved(
        game.answer_internal_card(
            choice.actor,
            choice.token,
            Answer::Ability(options[0].handle),
        )
        .unwrap(),
    )
}
