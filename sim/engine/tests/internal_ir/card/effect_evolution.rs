use super::*;
use sve_engine::game::internal::InputDomain;

fn tree(step: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut card = tree["cards"]["BP01-041"].clone();
    let header = &mut card["abilities"][0];
    header["activate"]["cost"] = json!({"pp":0});
    header["activate"]["do"] = Value::Array(vec![step]);
    tree["cards"] = json!({"unit-follower":card});
    tree
}

fn named_position(base: &WellFormedCardProgram) -> Game {
    let (mut snapshot, registry, document) = catalog_documents(&json!("1"));
    for (number, name) in [
        ("named-form", "named destination"),
        ("same-form", "unit-follower"),
    ] {
        snapshot.push('\n');
        snapshot.push_str(
            &json!({"number":number,"faces":[{
                "name":name,"card_class":"ニュートラル","card_type":"フォロワー・エボルヴ",
                "cost":"-","power":"4","hp":"5","traits":[],"text":null,"sections":[]
            }]})
            .to_string(),
        );
    }
    let forms = json!({"format":1_u8,"kind":"effect_set","cards":{
        "named-form":{"status":"complete","abilities":[]},
        "same-form":{"status":"complete","abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[
                ("units.yaml".into(), document),
                ("forms.yaml".into(), forms),
            ],
        )
        .unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"named","card":"named-form"},{"id":"decoy","card":"same-form"}
    ]);
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "named-evolution",
    )
    .unwrap();
    game.load_internal_card("unit-follower", base).unwrap();
    for number in ["named-form", "same-form", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}

#[test]
fn named_effect_evolution_selects_the_explicit_form_and_never_falls_back_to_same_name() {
    for destination in ["named destination", "absent destination"] {
        let programs = lowerer()
            .lower(&tree(json!({"evolve":{"ref":"this","into":destination}})))
            .unwrap();
        let mut game = named_position(&programs["unit-follower"]);
        let before = game.projection(View::Referee).unwrap();
        let CardPlay::Awaiting(question) = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap()
        else {
            panic!("private named evolution choice");
        };
        assert!(
            game.internal_decision(View::P2).is_none(),
            "the other player cannot inspect evolution resources"
        );
        let InputDomain::Targets(domain) = &question.domain else {
            panic!("named evolution domain");
        };
        let available = destination == "named destination";
        assert_eq!(
            domain.members.len(),
            usize::from(available),
            "a same-name decoy does not satisfy an explicit destination"
        );
        if available {
            assert_eq!(
                domain.members[0].card.as_deref(),
                Some("named-form"),
                "only the differently named printed destination is eligible"
            );
        } else {
            assert_eq!(
                domain.cardinality.minimum,
                Nat(0),
                "a missing form allows an empty resolution answer"
            );
        }
        let unchanged = format!("{game:?}");
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![u64::MAX]),
        )
        .unwrap_err();
        assert_eq!(
            format!("{game:?}"),
            unchanged,
            "an out-of-domain handle cannot change authority or state"
        );
        let activation = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Targets(domain.members.iter().map(|member| member.handle).collect()),
            )
            .unwrap(),
        );
        assert_eq!(
            activation
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Evolved(_)))
                .count(),
            usize::from(available),
            "only an actual named resource produces an evolution event"
        );
        let after = game.projection(View::Referee).unwrap();
        for resource in ["pp", "ep", "sep"] {
            assert_eq!(
                after["P1"][resource], before["P1"][resource],
                "effect evolution does not spend intrinsic {resource}"
            );
        }
        assert_eq!(
            after["objects"]["s"]["power"],
            if available { 4_u8 } else { 2_u8 },
            "the selected destination supplies the evolved characteristics"
        );
    }
}

#[test]
fn named_effect_evolution_rejects_forged_destinations_and_foreign_instruction_bindings() {
    let lowered = lowerer()
        .lower(&tree(
            json!({"evolve":{"ref":"this","into":"named destination"}}),
        ))
        .unwrap();
    let valid = lowered["unit-follower"].card();
    for names in [
        vec![],
        vec![
            CardName("named destination".into()),
            CardName("another destination".into()),
        ],
    ] {
        let mut forged = valid.clone();
        let Node::Control {
            control: Control::Sequence { body: outer },
        } = &mut forged.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("closed effect evolution projection");
        };
        let Node::Decision { decision } = &mut outer.steps[0].node else {
            panic!("private form decision");
        };
        let ChoiceDomain::Objects { constraint, .. } = &mut decision.domain else {
            panic!("form objects");
        };
        let Selector::Zone { predicate, .. } = &mut constraint.groups[0].selector else {
            panic!("evolve zone");
        };
        let Expr::Compute(operation) = predicate.as_mut() else {
            panic!("named form predicate");
        };
        let Predicate::NamesIntersect { right, .. } = operation.as_mut() else {
            panic!("literal destination names");
        };
        *right = Expr::Literal(names);
        assert!(
            WellFormedCardProgram::check(forged).is_err(),
            "empty or multi-name raw IR is outside the closed effect destination"
        );
    }
    let mut forged = valid.clone();
    let Node::Control {
        control: Control::Sequence { body: outer },
    } = &mut forged.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("closed effect evolution projection");
    };
    let Node::Control {
        control: Control::EachObject { body, .. },
    } = &mut outer.steps[1].node
    else {
        panic!("selected form iteration");
    };
    let scope = body.scope;
    let Node::Instruction { instruction } = &mut body.steps[0].node else {
        panic!("one evolution instruction");
    };
    let Action::Evolve { form, .. } = &mut instruction.action else {
        panic!("ordinary effect evolution");
    };
    form.object = ObjectOperand::Bound {
        binding: Binding::new(99, scope),
    };
    assert!(
        WellFormedCardProgram::check(forged).is_err(),
        "the instruction cannot consume a form binding different from its decision"
    );
}

#[test]
fn effect_evolution_uses_private_matching_resource_and_no_intrinsic_payment() {
    let lowered = lowerer()
        .lower(&tree(json!({"evolve":{"ref":"this"}})))
        .unwrap();
    let base = &lowered["unit-follower"];
    let mut game = evolution::position(1, base, &card(vec![]));
    let before = game.projection(View::Referee).unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("evolution form");
    };
    assert!(game.internal_decision(View::P2).is_none());
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("form domain");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("unit-form"));
    let atomic = format!("{game:?}");
    game.answer_internal_card(
        PlayerId { seat: 2 },
        question.token,
        Answer::Targets(vec![0]),
    )
    .unwrap_err();
    assert_eq!(format!("{game:?}"), atomic);
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
        .unwrap_err();
    assert_eq!(format!("{game:?}"), atomic);
    let activation = resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Evolved(_)))
            .count(),
        1
    );
    assert!(
        !activation
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::CardPlayed { .. }))
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["s"]["power"], 4);
    assert_eq!(after["objects"]["s"]["hp"], 5);
    assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
    assert_eq!(after["P1"]["ep"], before["P1"]["ep"]);
    assert_eq!(after["P1"]["sep"], before["P1"]["sep"]);
}

#[test]
fn effect_evolution_rejects_unneeded_shapes_and_forged_private_domain() {
    for step in [
        json!({"evolve":{"ref":"enemy"}}),
        json!({"gain_drive":{"ref":"this","into":"other"}}),
        json!({"evolve":{"ref":"this","into":["other"]}}),
        json!({"evolve":{"ref":"this","into":{"who":"self","zone":"evolve_deck","kind":"evolve_follower","name_contains":"other"}}}),
        json!({"evolve":{"ref":"this","into":"other","result":"changed"}}),
        json!({"evolve":{"ref":"this","into":"other","precondition":{"awakened":true}}}),
        json!({"evolve":{"ref":"this","into":""}}),
    ] {
        lowerer().lower(&tree(step)).unwrap_err();
    }
    let lowered = lowerer()
        .lower(&tree(json!({"evolve":{"ref":"this"}})))
        .unwrap();
    let valid = lowered["unit-follower"].card();
    for zone in [ZoneKind::Hand, ZoneKind::Deck, ZoneKind::Field] {
        let mut forged = valid.clone();
        let Node::Control {
            control: Control::Sequence { body },
        } = &mut forged.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("projection");
        };
        let Node::Decision { decision } = &mut body.steps[0].node else {
            panic!("decision");
        };
        let ChoiceDomain::Objects { constraint, .. } = &mut decision.domain else {
            panic!("objects");
        };
        let Selector::Zone { zone: selected, .. } = &mut constraint.groups[0].selector else {
            panic!("zone");
        };
        *selected = zone;
        WellFormedCardProgram::check(forged).unwrap_err();
    }
}

#[test]
fn effect_evolution_does_as_much_as_possible_when_a_later_form_is_unavailable() {
    let mut input = tree(json!({"evolve":{"ref":"this"}}));
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"evolve":{"ref":"this"}},{"evolve":{"ref":"this"}}
    ]);
    let lowered = lowerer().lower(&input).unwrap();
    let mut game = evolution::position(1, &lowered["unit-follower"], &card(vec![]));
    let CardPlay::Awaiting(first) = game
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("first evolution");
    };
    let CardPlay::Awaiting(second) = game
        .answer_internal_card(first.actor, first.token, Answer::Targets(vec![0]))
        .unwrap()
    else {
        panic!("second instruction");
    };
    let InputDomain::Targets(domain) = &second.domain else {
        panic!("empty form domain");
    };
    assert_eq!(domain.members.len(), 0);
    assert_eq!(domain.cardinality.minimum, Nat(0));
    let activation = resolved(
        game.answer_internal_card(second.actor, second.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    assert_eq!(
        activation
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::Evolved(_)))
            .count(),
        1
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
        4
    );
}

#[test]
fn effect_evolution_field_count_guard_uses_only_the_controllers_field() {
    for count in [2, 3] {
        let input = tree(
            json!({"if":{"cond":{"cmp":{"left":{"count":{"who":"self","zone":"field"}},"op":"eq","right":count}},"then":[{"evolve":{"ref":"this"}}]}}),
        );
        let lowered = lowerer().lower(&input).unwrap();
        let mut game = evolution::position(1, &lowered["unit-follower"], &card(vec![]));
        let outcome = game
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap();
        if count == 2 {
            assert!(matches!(outcome, CardPlay::Awaiting(_)));
        } else {
            resolved(outcome);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
                2
            );
        }
    }
    lowerer()
        .lower(&tree(
            json!({"if":{"cond":{"entered_from":{"ref":"this","from":"hand"}},"then":[]}}),
        ))
        .unwrap_err();
}
