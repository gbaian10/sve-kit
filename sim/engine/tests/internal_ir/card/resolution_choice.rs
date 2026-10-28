#![expect(
    clippy::panic,
    reason = "A missing decision fails the scenario immediately."
)]

use super::*;
use crate::rule_fixtures;
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn ex_catalog() -> Arc<Catalog> {
    let snapshot=[("unit-spell","スペル"),("unit-follower","フォロワー"),("lesson","スペル・トークン")].iter().map(|(number,kind)|json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":"1","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n");
    let document=json!({"format":1,"kind":"effect_set","cards":{
        "unit-spell":{"status":"complete","abilities":[]},"unit-follower":{"status":"complete","abilities":[]},"lesson":{"status":"complete","abilities":[]}
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
fn resolution_choice_rejects_hidden_sources_foreign_actors_and_optional_instructions() {
    for choice in [
        json!({"bind":"t","from":{"who":"opp","zone":"hand"},"n":1}),
        json!({"bind":"t","from":{"who":"opp","zone":"field"},"n":1,"by":"opp"}),
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
fn formal_targets_still_reject_ex_at_admission() {
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
    let initial = setup(1, json!([]));
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "formal-ex").unwrap();
    let before = game.projection(View::Referee).unwrap();
    assert!(matches!(
        game.load_internal_card("unit-spell", &card(vec![ability])),
        Err(sve_engine::EngineFailure::Unsupported(reason))
            if reason.contains("public field target selector")
    ));
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

#[test]
fn formal_targets_still_reject_ex_source_objects() {
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
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "formal-ex-source",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", &card(vec![ability]))
        .unwrap();
    let before = game.projection(View::Referee).unwrap();
    assert!(matches!(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 }),
        Err(sve_engine::EngineFailure::Unsupported(reason))
            if reason.contains("current public field object or EX card")
    ));
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}
