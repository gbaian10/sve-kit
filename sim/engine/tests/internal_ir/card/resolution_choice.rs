#![expect(
    clippy::panic,
    reason = "A missing decision fails the scenario immediately."
)]

use super::*;
use crate::rule_fixtures;
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::{DecisionView, InputDomain, SearchAnswer};

fn ex_catalog() -> Arc<Catalog> {
    let snapshot=[("unit-spell","スペル"),("unit-follower","フォロワー"),("unit-amulet","アミュレット"),("lesson","スペル・トークン")].iter().map(|(number,kind)|json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":if *kind=="アミュレット" {"-"} else {"2"},"hp":if *kind=="アミュレット" {"-"} else {"3"},"traits":[],"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n");
    let document=json!({"format":1,"kind":"effect_set","cards":{
        "unit-spell":{"status":"complete","abilities":[]},"unit-follower":{"status":"complete","abilities":[]},"unit-amulet":{"status":"complete","abilities":[]},"lesson":{"status":"complete","abilities":[]}
    }}).to_string();
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([]),
        json!([rule_fixtures::resource(
            "lesson_item",
            "lesson",
            "printed",
            true
        )]),
    )
    .to_string();
    Arc::new(
        Catalog::from_documents_with_rules(
            &snapshot,
            "format: 1\nkind: keyword_registry\nkeywords: {}\n",
            &[("ex.yaml".into(), document)],
            &rules,
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap(),
    )
}

fn tree(choice: &Value, action: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry["activate"] = json!({"cost":{"pp":1},"do":[{"choose":choice},action]});
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({"unit-follower":{"meta":meta,"abilities":[entry]}});
    tree
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("resolution choice")
    };
    question
}

#[test]
fn resolution_exact_choice_uses_available_field_objects_and_keeps_payment() {
    let programs = lowerer()
        .lower(&tree(
            &json!({"bind":"t","from":{"who":"opp","zone":"field","kind":"follower"},"n":2}),
            &json!({"damage":{"to":"t","n":1}}),
        ))
        .unwrap();
    for count in [0_u64, 1] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!(
            (0..count)
                .map(|n| json!({"id":format!("enemy{n}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "resolution-count",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        let question = awaiting(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("field choice")
        };
        assert_eq!(domain.members.len(), usize::try_from(count).unwrap());
        assert_eq!(domain.cardinality.minimum, Nat(count));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            9
        );
        let before = format!("{game:?}");
        game.answer_internal_card(
            PlayerId { seat: 2 },
            question.token,
            Answer::Targets(vec![]),
        )
        .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "invalid answers preserve authority state"
        );
        if count == 1 {
            game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
                .unwrap_err();
            assert!(
                format!("{game:?}") == before,
                "an available required object cannot be omitted"
            );
        }
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets((0..count).collect()),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 9);
        if count == 1 {
            assert_eq!(state["objects"]["enemy0"]["damage"], 1);
        }
    }
}

#[test]
fn resolution_amulet_choice_reuses_public_handles_and_destroy() {
    let programs = lowerer()
        .lower(&tree(
            &json!({"bind":"t","from":{"who":"opp","zone":"field","kind":"amulet"},"n":1}),
            &json!({"destroy":"t"}),
        ))
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"unit-follower"},{"id":"amulet","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "resolution-amulet",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    let question = awaiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(matches!(&question.domain, InputDomain::Targets(domain) if domain.members.len() == 1));
    let before = format!("{game:?}");
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![99]))
        .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "invalid answers preserve authority state"
    );
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["enemy"]));
    assert_eq!(state["P2"]["cemetery"], json!(["amulet"]));
}

#[test]
fn resolution_choice_rejects_hidden_sources_invalid_actors_and_optional_instructions() {
    for choice in [
        json!({"bind":"t","from":{"who":"opp","zone":"hand"},"n":1}),
        json!({"bind":"t","from":{"who":"opp","zone":"field"},"n":1,"by":"turn"}),
        json!({"bind":"t","from":{"who":"opp","zone":"field"},"n":1,"may":true}),
        json!({"bind":"t","from":{"who":"opp","zone":"field"},"n":0}),
    ] {
        lowerer()
            .lower(&tree(&choice, &json!({"destroy":"t"})))
            .unwrap_err();
    }
}

#[test]
fn resolution_choice_cannot_replace_an_existing_observation_binding() {
    let choice = json!({"bind":"t","from":{"who":"opp","zone":"field"},"n":1});
    let mut duplicate = tree(&choice, &json!({"destroy":"t"}));
    duplicate["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"look":{"top":{"who":"self","n":1},"bind":"t"}},
        {"choose":choice},
        {"destroy":"t"}
    ]);
    let error = lowerer().lower(&duplicate).unwrap_err();
    assert_eq!(error.code, "unsupported_construct");
}

#[test]
fn named_field_choices_reject_shared_names_and_accept_distinct_names() {
    for (substring, expected_count, handles, remaining) in [
        ("unit-", 3, vec![0, 2], json!(["duplicate"])),
        ("follower", 2, vec![0], json!(["duplicate", "different"])),
    ] {
        let programs = lowerer().lower(&tree(
        &json!({"bind":"t","from":{"who":"opp","zone":"field","name_contains":substring},"upto":2,"distinct_by":"name"}),
        &json!({"destroy":"t"}),
    )).unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"first","card":"unit-follower"}, {"id":"duplicate","card":"unit-follower"}, {"id":"different","card":"unit-amulet"}
        ]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "distinct-names",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        let question = awaiting(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("named targets");
        };
        assert_eq!(domain.members.len(), expected_count);
        assert!(domain.distinct_names);
        let before = format!("{game:?}");
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 1]))
            .unwrap_err();
        assert!(
            format!("{game:?}") == before,
            "invalid named selection is atomic"
        );
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], remaining);
    }
}

#[test]
fn exact_distinct_name_resolution_uses_the_largest_compatible_selection() {
    let programs = lowerer().lower(&tree(
        &json!({"bind":"t","from":{"who":"opp","zone":"field","name":"unit-follower"},"n":2,"distinct_by":"name"}),
        &json!({"damage":{"to":"t","n":1}}),
    )).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"first","card":"unit-follower"},{"id":"second","card":"unit-follower"},{"id":"unrelated","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "distinct-shortage",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    let question = awaiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("named targets");
    };
    assert_eq!(
        domain.members.len(),
        2,
        "exact names exclude the unrelated amulet"
    );
    assert_eq!(domain.cardinality.minimum, Nat(1));
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
}

#[test]
fn named_field_collection_buffs_only_matching_objects() {
    let action =
        json!({"buff":{"on":{"who":"opp","zone":"field","name_contains":"follower"},"power":1}});
    let mut input = tree(
        &json!({"bind":"unused","from":{"who":"opp","zone":"field"},"n":1}),
        &action,
    );
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([action]);
    let programs = lowerer().lower(&input).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"unit-follower"},{"id":"amulet","card":"unit-amulet"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "named-collection",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &programs["unit-follower"])
        .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    let before = game.projection(View::Referee).unwrap()["objects"]["amulet"].clone();
    resolved(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["power"], 3);
    assert_eq!(state["objects"]["s"]["power"], 2);
    assert_eq!(state["objects"]["amulet"], before);
}

#[test]
fn ex_choice_accepts_public_cards_and_rejects_nonfield_damage() {
    let choice = json!({"bind":"t","from":{"who":"opp","zone":"ex"},"n":1});
    let lowerer = lowerer();
    for damage in [false, true] {
        let action = if damage {
            json!({"damage":{"to":"t","n":1}})
        } else {
            json!({"banish":"t"})
        };
        let programs = lowerer.lower(&tree(&choice, &action)).unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"own","card":"unit-spell"}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"ex-spell","card":"unit-spell"},{"id":"ex-follower","card":"unit-follower"}]);
        let mut game = Game::new(
            ex_catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "ex-choice",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let initial_state = game.projection(View::Referee).unwrap();
        let question = awaiting(
            game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("EX choice")
        };
        assert_eq!(domain.members.len(), 2);
        assert!(domain.members.iter().all(|member| member.player.seat == 2));
        let handle = domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some("unit-spell"))
            .unwrap()
            .handle;
        let answer = game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![handle]),
        );
        if damage {
            assert!(
                matches!(answer, Err(sve_engine::EngineFailure::Unsupported(reason)) if reason.contains("non-field damage capture"))
            );
            assert_eq!(game.projection(View::Referee).unwrap(), initial_state);
        } else {
            resolved(answer.unwrap());
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P2"]["banish"], json!(["ex-spell"]));
            assert_eq!(state["P2"]["ex"], json!(["ex-follower"]));
            assert_eq!(state["P1"]["ex"], json!(["own"]));
        }
    }
    for zone in ["hand", "deck"] {
        let mut hidden = choice.clone();
        hidden["from"]["zone"] = json!(zone);
        lowerer
            .lower(&tree(&hidden, &json!({"banish":"t"})))
            .unwrap_err();
    }
}

#[test]
fn formal_targets_admit_and_select_public_ex_cards_before_payment() {
    let mut target = play::target(10, 1, 1, 1);
    let ChoiceDomain::Objects { constraint, .. } = &mut target.domain else {
        panic!("object domain");
    };
    let Selector::Zone { zone, .. } = &mut constraint.groups[0].selector else {
        panic!("zone selector");
    };
    *zone = ZoneKind::Ex;
    let mut ability = play::spell(vec![]);
    ability.node.play.as_mut().unwrap().parameters = vec![target];
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"ex-card","card":"unit-follower"}]);
    let mut game = Game::new(
        ex_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "formal-ex",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &card(vec![ability]))
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    let question = awaiting(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal EX target");
    };
    assert_eq!(domain.members.len(), 1);
    let member = domain.members.first().unwrap();
    assert_eq!(member.player.seat, 2);
    assert_eq!(member.card.as_deref(), Some("unit-follower"));
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["pp"]["current"], 2_u8);
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![u64::MAX]),
    )
    .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![member.handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 1_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["s"]));
    assert_eq!(state["P2"]["ex"], json!(["ex-card"]));
    assert_eq!(state["P2"]["field"], json!(["f"]));
}

#[test]
fn formal_targets_can_select_their_ex_source_holder() {
    let mut target = play::target(10, 1, 1, 1);
    let ChoiceDomain::Objects { constraint, .. } = &mut target.domain else {
        panic!("object domain");
    };
    constraint.groups[0].selector = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    let mut ability = payment::paid_ability(vec![payment::pp(1)], vec![]);
    ability.source = source("unit-amulet");
    ability.source.fragment = None;
    ability.node.active_zones = vec![ZoneKind::Ex];
    ability.node.play.as_mut().unwrap().parameters = vec![target];
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"s","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
    let mut game = Game::new(
        ex_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "formal-ex-source",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![ability]))
        .unwrap();
    let question = awaiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("formal EX source target");
    };
    assert_eq!(domain.members.len(), 1);
    let member = domain.members.first().unwrap();
    assert_eq!(member.player.seat, 1);
    assert_eq!(member.card.as_deref(), Some("unit-amulet"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4_u8
    );
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![member.handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 3_u8);
    assert_eq!(state["P1"]["ex"], json!(["s"]));
}

#[test]
fn optional_body_can_choose_targets_but_cannot_export_a_conditional_binding() {
    let mut input = tree(
        &json!({"bind":"outer","from":{"who":"opp","zone":"field","kind":"follower"},"upto":1}),
        &json!({"optional":{"do":[
            {"choose":{"bind":"inner","from":{"who":"opp","zone":"field","kind":"follower"},"upto":1}},
            {"damage":{"to":"inner","n":1}}
        ]}}),
    );
    lowerer().lower(&input).unwrap();
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .push(json!({"damage":{"to":"inner","n":1}}));
    assert!(
        lowerer().lower(&input).is_err(),
        "declined optional body cannot provide an outer binding"
    );
}

#[test]
fn optional_resolution_skips_or_runs_nested_target_and_alternative_decisions() {
    let mut input = tree(
        &json!({"bind":"outer","from":{"who":"opp","zone":"field","kind":"follower"},"upto":1}),
        &json!({"optional":{"do":[
            {"choose":{"bind":"inner","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}}
        ],"then":[
            {"either":{"options":[
                [{"damage":{"to":"inner","n":1}}],
                [{"damage":{"to":"inner","n":2}}]
            ]}}
        ]}}),
    );
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    let programs = lowerer().lower(&input).unwrap();
    for accept in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "nested-optional",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        let choice = awaiting(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let mut play = game
            .answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Integer(Nat(u64::from(accept))),
            )
            .unwrap();
        if accept {
            let question = awaiting(play);
            let InputDomain::Targets(domain) = question.domain else {
                panic!("inner target")
            };
            assert_eq!(domain.members.len(), 1);
            let modes = awaiting(
                game.answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::Targets(vec![domain.members[0].handle]),
                )
                .unwrap(),
            );
            play = game
                .answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(2)]))
                .unwrap();
        }
        assert!(matches!(play, CardPlay::Resolved(_)));
        assert!(game.internal_decision(View::Referee).is_none());
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            u8::from(accept) * 2
        );
    }
}

#[test]
fn optional_search_is_asked_only_after_acceptance_and_keeps_the_outer_continuation() {
    let mut input = tree(
        &json!({"bind":"unused","from":{"who":"opp","zone":"field","kind":"follower"},"upto":1}),
        &json!({"optional":{"do":[{"search":{"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1,"to":"hand"}}]}}),
    );
    let steps = input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap();
    steps.remove(0);
    steps.push(json!({"life":{"who":"self","delta":1}}));
    let programs = lowerer().lower(&input).unwrap();
    for accept in [false, true] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["leader"]["life"] = json!(10);
        initial["players"]["P1"]["pp"] = json!({"current":10,"max":10});
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "optional-search",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        let choice = awaiting(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let mut play = game
            .answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Integer(Nat(u64::from(accept))),
            )
            .unwrap();
        if accept {
            let search = awaiting(play);
            assert!(matches!(search.domain, InputDomain::Search(_)));
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                10
            );
            play = game
                .answer_internal_card(
                    search.actor,
                    search.token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![0])),
                )
                .unwrap();
        }
        assert!(matches!(play, CardPlay::Resolved(_)));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 11);
        assert_eq!(
            state["P1"]["hand"],
            if accept { json!(["found"]) } else { json!([]) }
        );
        assert_eq!(state["P1"]["pp"]["current"], 9);
    }
}

#[test]
fn optional_can_move_an_inherited_look_receipt_but_cannot_invent_one() {
    let mut document = tree(
        &json!({"bind":"target","from":{"who":"opp","zone":"field","kind":"follower"},"upto":1}),
        &json!({"life":{"who":"self","delta":1}}),
    );
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"look":{"top":{"who":"self","n":1},"bind":"seen"}},
        {"optional":{"do":[{"move":{"ref":{"in":"seen"},"to":"cemetery"}}]}}
    ]);
    lowerer().lower(&document).unwrap();
    let mut foreign = document.clone();
    foreign["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"destroy":{"ref":{"who":"opp","zone":"field","kind":"follower"},"result":"moved"}},
        {"optional":{"do":[{"move":{"ref":{"in":{"objects_of":"moved","view":"before"}},"to":"cemetery"}}]}}
    ]);
    let mut producer_only = foreign.clone();
    producer_only["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .pop();
    lowerer().lower(&producer_only).unwrap();
    lowerer().lower(&foreign).unwrap_err();
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1]["optional"]["do"][0]
        ["move"]["ref"]["in"] = json!("unseen");
    lowerer().lower(&document).unwrap_err();
}

#[test]
fn optional_counter_change_is_visible_to_a_later_counter_read() {
    let mut input = tree(
        &json!({"bind":"unused","from":{"who":"opp","zone":"field","kind":"follower"},"upto":1}),
        &json!({"optional":{"do":[{"counter":{"on":"this","name":"test","delta":2}}]}}),
    );
    let body = input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap();
    body.remove(0);
    body.push(json!({"draw":{"who":"self","n":{"counter":{"on":"this","name":"test"}}}}));
    let programs = lowerer().lower(&input).unwrap();
    for accept in [false, true] {
        let mut initial = setup(
            1,
            json!([
                {"id":"first","card":"unit-spell"},
                {"id":"second","card":"unit-spell"},
                {"id":"third","card":"unit-spell"}
            ]),
        );
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"source","card":"unit-follower","state":{"counters":{"test":1}}}
        ]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "optional-counter-integration",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &programs["unit-follower"])
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let choice = awaiting(
            game.activate_internal_card("source", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        resolved(
            game.answer_internal_card(
                choice.actor,
                choice.token,
                Answer::Integer(Nat(u64::from(accept))),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        let expected = if accept { 3_u64 } else { 1 };
        assert_eq!(state["objects"]["source"]["counters"]["test"], expected);
        assert_eq!(
            state["P1"]["hand"].as_array().unwrap().len(),
            usize::try_from(expected).unwrap()
        );
    }
}
