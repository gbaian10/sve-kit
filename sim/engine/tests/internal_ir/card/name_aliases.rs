use super::*;
use sve_engine::game::internal::{Answer, InputDomain, SearchAnswer};

fn program(head: &Value, number: &str) -> WellFormedCardProgram {
    lowerer()
        .lower(&same_name::tree(head, number))
        .unwrap()
        .remove(number)
        .unwrap()
}
fn alias(field_only: bool) -> Value {
    let mut head = json!({"static":{"effects":[{"also_named":{"name":"unit-spell"}}]}});
    if field_only {
        head["static"]["zone"] = json!("field");
    }
    head
}
fn position(zone: &str, field_only: bool, caster: &Value, silenced: bool) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"caster","card":"unit-amulet"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    if initial["players"]["P1"]["zones"][zone].is_null() {
        initial["players"]["P1"]["zones"][zone] = json!([]);
    }
    initial["players"]["P1"]["zones"][zone]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"alias","card":"unit-follower","state":{"silenced":silenced}}));
    if zone == "deck" {
        initial["players"]["P1"]["zones"]["deck"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"other","card":"unit-spell"}));
    }
    if zone != "deck" {
        initial["players"]["P1"]["zones"]["deck"] = json!(
            (0..4)
                .map(|n| json!({"id":format!("draw{n}"),"card":"unit-spell"}))
                .collect::<Vec<_>>()
        );
    }
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "names").unwrap();
    game.load_internal_card(
        "unit-follower",
        &program(&alias(field_only), "unit-follower"),
    )
    .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-amulet", &program(caster, "unit-amulet"))
        .unwrap();
    game
}
fn activate(game: &mut Game) -> CardPlay {
    game.activate_internal_card("caster", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
}

#[test]
fn exact_and_substring_filters_preserve_original_name_and_respect_zone_and_silence() {
    for (zone, field_only, silenced, expected) in [
        ("field", true, false, 1),
        ("field", true, true, 0),
        ("ex", true, false, 0),
        ("cemetery", true, false, 0),
        ("ex", false, false, 1),
        ("cemetery", false, false, 1),
    ] {
        for filter in [
            json!({"name":"unit-spell"}),
            json!({"name_contains":"spell"}),
            json!({"name":"unit-follower"}),
        ] {
            let original = filter.get("name") == Some(&json!("unit-follower"));
            let mut from = json!({"who":"self","zone":zone,"kind":"follower"});
            from.as_object_mut()
                .unwrap()
                .extend(filter.as_object().unwrap().clone());
            let caster = json!({"activate":{"cost":{"pp":0},"targets":[{"bind":"picked","from":from,"upto":1}],"do":[{"life":{"who":"self","delta":1}}]}});
            let mut game = position(zone, field_only, &caster, silenced);
            let CardPlay::Awaiting(q) = activate(&mut game) else {
                panic!("target choice");
            };
            let InputDomain::Targets(d) = q.domain else {
                panic!("target domain");
            };
            assert_eq!(
                d.members.len(),
                if original { 1 } else { expected },
                "{zone}/{field_only}/{silenced}"
            );
        }
    }
}

#[test]
fn search_reads_always_names_and_rejects_two_cards_with_a_shared_alias() {
    let caster = json!({"activate":{"cost":{"pp":0},"do":[{"search":{"sel":{"who":"self","zone":"deck"},"upto":2,"distinct_by":"name","to":"hand"}}]}});
    for field_only in [false, true] {
        let mut game = position("deck", field_only, &caster, false);
        let CardPlay::Awaiting(q) = activate(&mut game) else {
            panic!("search");
        };
        let InputDomain::Search(d) = &q.domain else {
            panic!("search domain");
        };
        assert_eq!(d.cards.len(), 2);
        let answer = SearchAnswer::Grouped(vec![d.cards.iter().map(|c| c.handle).collect()]);
        let result = game.answer_internal_card(q.actor, q.token, Answer::Search(answer));
        if field_only {
            resolved(result.unwrap());
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view["P1"]["hand"].as_array().unwrap().len(), 2);
        } else {
            assert!(
                matches!(result, Err(sve_engine::EngineFailure::Invalid(reason))
                if reason.contains("search names must differ"))
            );
        }
    }
}

#[test]
fn alias_declarations_reject_unneeded_targets_conditions_zones_and_mixed_effects() {
    for head in [
        json!({"static":{"effects":[{"also_named":{"name":"extra","on":{"who":"self","zone":"field"}}}]}}),
        json!({"static":{"while":{"awakened":true},"effects":[{"also_named":{"name":"extra"}}]}}),
        json!({"static":{"zone":"hand","effects":[{"also_named":{"name":"extra"}}]}}),
        json!({"static":{"effects":[{"also_named":{"name":"extra"}},{"grant":{"keywords":["ward"]}}]}}),
    ] {
        assert_eq!(
            lowerer()
                .lower(&same_name::tree(&head, "unit-follower"))
                .unwrap_err()
                .code,
            "unsupported_construct"
        );
    }
}

#[test]
fn distinct_name_count_includes_the_additional_name_once() {
    let caster = json!({"activate":{"cost":{"pp":0},"do":[{"draw":{"n":{"distinct_names":{"who":"self","zone":"field"}}}}]}});
    for (silenced, expected) in [(false, 3), (true, 2)] {
        let mut game = position("field", true, &caster, silenced);
        resolved(activate(&mut game));
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"]
                .as_array()
                .unwrap()
                .len(),
            expected
        );
    }
}
#[test]
fn private_search_exact_and_substring_filters_share_the_zone_sensitive_names() {
    for field_only in [true, false] {
        for filter in [
            json!({"name":"unit-spell"}),
            json!({"name_contains":"spell"}),
        ] {
            let mut sel = json!({"who":"self","zone":"deck"});
            sel.as_object_mut()
                .unwrap()
                .extend(filter.as_object().unwrap().clone());
            let caster = json!({"activate":{"cost":{"pp":0},"do":[{"search":{"sel":sel,"upto":1,"to":"hand"}}]}});
            let mut game = position("deck", field_only, &caster, false);
            let CardPlay::Awaiting(q) = activate(&mut game) else {
                panic!("search");
            };
            let InputDomain::Search(d) = q.domain else {
                panic!("search domain");
            };
            assert_eq!(d.cards.len(), if field_only { 1 } else { 2 });
        }
    }
}

#[test]
fn differently_printed_evolution_uses_the_alias_and_its_association_survives_departure() {
    let base = program(
        &json!({"evolve":{"cost":{"pp":0},"into":"unit-spell"}}),
        "unit-follower",
    );
    for field_only in [false, true] {
        let mut tree = same_name::tree(&alias(field_only), "unit-form");
        let mut departure = tree["cards"]["unit-form"]["abilities"][0].clone();
        departure.as_object_mut().unwrap().remove("static");
        departure["line"] = json!(3);
        departure["activate"] =
            json!({"cost":{"pp":0},"do":[{"move":{"ref":"this","to":"cemetery"}}]});
        tree["cards"]["unit-form"]["abilities"]
            .as_array_mut()
            .unwrap()
            .push(departure);
        let form = lowerer().lower(&tree).unwrap().remove("unit-form").unwrap();
        let definition = form
            .card()
            .abilities
            .iter()
            .find(|ability| ability.node.kind == AbilityKind::Activated)
            .unwrap()
            .node
            .id;
        let mut game = evolution::position_with_form_name(1, &base, &form, "different-form");
        let result = game.internal_rule_action(
            &json!({"do":"evolve","source":"s","evolve_card":"e","pay":{"pp":0}}),
        );
        if field_only {
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["s"]["evolved"],
                false
            );
            assert!(
                game.projection(View::Referee).unwrap()["P1"]["evolve_deck"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("e"))
            );
        } else {
            result.unwrap();
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["s"]["evolved"],
                true
            );
            while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            resolved(
                game.activate_internal_card("s", definition, PlayerId { seat: 1 })
                    .unwrap(),
            );
            let view = game.projection(View::Referee).unwrap();
            assert!(
                view["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("s"))
            );
            assert!(
                view["P1"]["evolve_deck"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("e"))
            );
            assert_eq!(view["objects"]["e"]["face_up"], true);
        }
    }
}

#[test]
fn departure_observers_use_the_alias_from_before_exit() {
    let observer = json!({"auto":{"on":{"left_field":{"who":"self","sel":{"name":"unit-spell"}}},"do":[{"draw":{"n":1}}]}});
    let mut caster = same_name::tree(&observer, "unit-amulet");
    let mut activation = caster["cards"]["unit-amulet"]["abilities"][0].clone();
    activation.as_object_mut().unwrap().remove("auto");
    activation["line"] = json!(3);
    activation["activate"] = json!({"cost":{"pp":0},"do":[{"move":{"ref":{"who":"self","zone":"field","name":"unit-spell"},"to":"cemetery"}}]});
    caster["cards"]["unit-amulet"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(activation);
    let caster_program = lowerer()
        .lower(&caster)
        .unwrap()
        .remove("unit-amulet")
        .unwrap();
    let mut initial = setup(1, json!([{"id":"drawn","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"caster","card":"unit-amulet"},{"id":"alias","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "alias-lki").unwrap();
    game.load_internal_card("unit-amulet", &caster_program)
        .unwrap();
    game.load_internal_card("unit-follower", &program(&alias(true), "unit-follower"))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let definition = caster_program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    resolved(
        game.activate_internal_card("caster", definition, PlayerId { seat: 1 })
            .unwrap(),
    );
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = q.domain else {
        panic!("departure observer");
    };
    assert_eq!(options.len(), 1);
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["drawn"])
    );
}

#[test]
fn a_printed_search_match_is_conclusive_while_unknown_additional_names_remain_unavailable() {
    for name in ["unit-follower", "missing-name"] {
        let caster = program(
            &json!({"activate":{"cost":{"pp":0},"do":[{"search":{"sel":{"who":"self","zone":"deck","name":name},"upto":1,"to":"hand"}}]}}),
            "unit-amulet",
        );
        let mut initial = setup(1, json!([{"id":"unknown","card":"unit-follower"}]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"caster","card":"unit-amulet"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "unknown-names",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &caster).unwrap();
        let result = game.activate_internal_card("caster", AbilityId(1), PlayerId { seat: 1 });
        if name == "unit-follower" {
            let CardPlay::Awaiting(q) = result.unwrap() else {
                panic!("search");
            };
            let InputDomain::Search(domain) = q.domain else {
                panic!("search domain");
            };
            assert_eq!(domain.cards.len(), 1);
        } else {
            assert!(
                matches!(result,Err(sve_engine::EngineFailure::Unsupported(reason)) if reason.contains("complete card names"))
            );
        }
    }
}

#[test]
fn public_look_distinct_names_keep_one_rule_label_and_reject_departed_members() {
    for moved in [false, true] {
        let mut steps = vec![json!({"reveal":{"top":{"who":"self","n":1},"bind":"seen"}})];
        if moved {
            steps.push(json!({"move":{"ref":{"in":"seen"},"to":"hand"}}));
        }
        steps.push(json!({"life":{"who":"self","delta":{"distinct_names":{"in":{"objects_of":"seen","view":"before"}}}}}));
        let caster = json!({"activate":{"cost":{"pp":0},"do":steps}});
        let mut game = position("deck", false, &caster, false);
        let before = game.projection(View::Referee).unwrap();
        let result = game.activate_internal_card("caster", AbilityId(1), PlayerId { seat: 1 });
        if moved {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("stale public look member")
            );
            assert_eq!(game.projection(View::Referee).unwrap(), before);
        } else {
            resolved(result.unwrap());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                21,
                "a revealed card's additional alias is not a second rule label"
            );
        }
    }
}

#[test]
fn receipt_scalar_does_not_turn_a_private_look_into_a_public_observation() {
    let public = json!({"activate":{"cost":{"pp":0},"do":[
        {"reveal":{"top":{"who":"self","n":1},"bind":"seen"}},
        {"life":{"who":"self","delta":{"distinct_names":{"in":{"objects_of":"seen","view":"before"}}}}}
    ]}});
    let mut private = public.clone();
    private["activate"]["do"][0] = json!({"look":{"top":{"who":"self","n":1},"bind":"seen"}});
    lowerer()
        .lower(&same_name::tree(&private, "unit-amulet"))
        .unwrap_err();
    let mut raw = program(&public, "unit-amulet").card().clone();
    raw.abilities[0].node.body.as_mut().unwrap().steps.retain(|step| {
        !matches!(&step.node, Node::Instruction { instruction } if matches!(instruction.action, Action::Reveal { .. }))
    });
    let checked = WellFormedCardProgram::check(raw).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "private-look-scalar",
    )
    .unwrap();
    let error = game
        .load_internal_card("unit-amulet", &checked)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("distinct names before revelation")
    );
}

#[test]
fn player_question_search_exposes_complete_rules_names_but_no_unknown_deck_links() {
    use crate::player_pipeline::{answer, choose_search, question};
    use sve_engine::game::player::{DecisionShape, Input, QuestionShape, SubmissionKind, Viewer};
    let caster = json!({"activate":{"cost":{"pp":0},"do":[{"search":{"sel":{"who":"self","zone":"deck"},"upto":2,"distinct_by":"name","to":"hand"}}]}});
    let mut game = position("deck", false, &caster, false);
    activate(&mut game);
    let public = question(&game, Viewer::P1);
    let QuestionShape::Decision(DecisionShape::Search(search)) = &public.shape else {
        panic!("public search");
    };
    assert!(search.distinct_names);
    assert!(search.cards.iter().all(|card| card.object.is_none()));
    let follower = search
        .cards
        .iter()
        .find(|card| card.card == "unit-follower")
        .unwrap();
    assert_eq!(follower.rules_names.as_ref().unwrap().len(), 2);
    assert!(
        follower
            .rules_names
            .as_ref()
            .unwrap()
            .contains(&"unit-spell".into())
    );
    assert!(matches!(game.input(Viewer::P2), Input::Waiting(_)));
    let before = exact_authority(&game);
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Search(SearchAnswer::Grouped(vec![
                search.cards.iter().map(|card| card.handle).collect()
            ]))
        )
        .kind,
        SubmissionKind::Rejected
    );
    assert_eq!(exact_authority(&game), before);
    assert_eq!(question(&game, Viewer::P1), public);
    assert_eq!(
        answer(&mut game, Viewer::P1, choose_search(search)).kind,
        SubmissionKind::Applied
    );
}
