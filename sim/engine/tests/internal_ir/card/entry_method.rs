use super::*;
use sve_engine::game::internal::InputDomain;

fn candidate(by: &str) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut definition = tree["cards"]["BP08-038"].clone();
    let mut ability = definition["abilities"][0].clone();
    ability["fanfare"] = json!({"do":[{"if":{
        "cond":{"entered_by":{"ref":"this","by":by}},
        "then":[{"draw":{"n":1}}]
    }}]});
    definition["abilities"] = json!([ability]);
    tree["cards"] = json!({"unit-follower":definition});
    tree
}

#[test]
fn ability_entry_condition_lowers_both_registered_methods() {
    for by in ["ability", "play"] {
        lowerer().lower(&candidate(by)).unwrap();
    }
}

fn program(by: &str, origin: ZoneKind, automatic: bool) -> WellFormedCardProgram {
    let mut candidate = candidate(by);
    if automatic {
        let entry = &mut candidate["cards"]["unit-follower"]["abilities"][0];
        let mut body = entry.as_object_mut().unwrap().remove("fanfare").unwrap();
        body["on"] = json!({"enter_field":{"ref":"this"}});
        entry["auto"] = body;
    }
    let cards = lowerer().lower(&candidate).unwrap();
    let mut trigger = cards["unit-follower"].card().abilities[0].clone();
    trigger.source = source("unit-follower");
    trigger.source.fragment = None;
    let printed = AbilityOrigin::Printed {
        face: FaceId("0".into()),
        line: Nat(1),
    };
    match &mut trigger.node.source.origin {
        AbilityOrigin::Keyword { origin, .. } => **origin = printed,
        origin @ (AbilityOrigin::Printed { .. } | AbilityOrigin::Granted { .. }) => {
            *origin = printed;
        }
    }
    locate_steps(
        &mut trigger.node.body.as_mut().unwrap().steps,
        &trigger.source,
        &trigger.node.source,
    );
    card(vec![
        trigger,
        ability(9, vec![entry::enter(90, entry::own_zone(origin, 9))]),
    ])
}

#[test]
fn entry_method_condition_distinguishes_play_from_ability_put_in_the_same_zone() {
    for (by, automatic) in [
        ("ability", false),
        ("play", false),
        ("ability", true),
        ("play", true),
    ] {
        for (from, method) in [
            ("hand", true),
            ("ex", true),
            ("cemetery", false),
            ("ex", false),
        ] {
            let zone = match from {
                "cemetery" => ZoneKind::Cemetery,
                _ => ZoneKind::Ex,
            };
            let mut initial = setup(1, json!([]));
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
            initial["players"]["P1"]["zones"][from] = json!([{"id":"incoming","card":"unit-follower","state":{"entered_by":"ability","entered_by_effect":true}}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"unit-spell"}]);
            let mut game = Game::new(
                catalog(),
                &initial,
                &Value::Null,
                &Value::Null,
                "entry-method",
            )
            .unwrap();
            game.load_internal_card("unit-follower", &program(by, zone, automatic))
                .unwrap();
            game.load_internal_card("unit-spell", &card(vec![]))
                .unwrap();
            if method {
                assert!(
                    matches!(
                        game.play_internal_card("incoming", PlayerId { seat: 1 })
                            .unwrap(),
                        CardPlay::Entered(_)
                    ),
                    "normal play enters"
                );
            } else {
                resolved(
                    game.activate_internal_card("s", AbilityId(9), PlayerId { seat: 1 })
                        .unwrap(),
                );
            }
            if !method {
                game.resume_internal_card().unwrap();
            }
            let question = game.internal_pending_choice().unwrap().unwrap();
            resolved(
                game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
                    .unwrap(),
            );
            let matches = (by == "play") == method;
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["hand"],
                if matches { json!(["drawn"]) } else { json!([]) },
                "the matched entry event determines the branch even with spoofed object flags"
            );
        }
    }
}

#[test]
fn entry_method_condition_rejects_unknown_methods_and_nonentry_contexts() {
    assert!(
        lowerer().lower(&candidate("rule")).is_err(),
        "unknown author entry methods remain closed"
    );
    let mut tree = candidate("ability");
    let entry = &mut tree["cards"]["unit-follower"]["abilities"][0];
    let body = entry.as_object_mut().unwrap().remove("fanfare").unwrap();
    entry["activate"] = body;
    entry["activate"]["cost"] = json!({"pp":0});
    let mut supported = tree.clone();
    supported["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] =
        json!([{"draw":{"n":1}}]);
    lowerer().lower(&supported).unwrap();
    assert!(
        lowerer().lower(&tree).is_err(),
        "an activated ability has no matched entry event"
    );
}

#[test]
fn entry_method_condition_keeps_the_entry_fact_after_its_source_leaves() {
    let mut tree = candidate("ability");
    let mut departure = tree["cards"]["unit-follower"]["abilities"][0].clone();
    departure["line"] = json!(2);
    departure["fanfare"] = json!({"do":[{"move":{"ref":"this","to":"cemetery"}}]});
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(departure);
    let cards = lowerer().lower(&tree).unwrap();
    let mut abilities = cards["unit-follower"].card().abilities.clone();
    for (entry, line) in abilities.iter_mut().zip(1_u64..) {
        entry.source = source("unit-follower");
        entry.source.fragment = None;
        entry.source.line = u32::try_from(line).unwrap();
        if let AbilityOrigin::Keyword { origin, .. } = &mut entry.node.source.origin {
            **origin = AbilityOrigin::Printed {
                face: FaceId("0".into()),
                line: Nat(line),
            };
        }
        locate_steps(
            &mut entry.node.body.as_mut().unwrap().steps,
            &entry.source,
            &entry.node.source,
        );
    }
    abilities.push(ability(
        9,
        vec![entry::enter(90, entry::own_zone(ZoneKind::Cemetery, 9))],
    ));
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"incoming","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"unit-spell"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "entry-frozen",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &card(abilities))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    resolved(
        game.activate_internal_card("s", AbilityId(9), PlayerId { seat: 1 })
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("missing pending entry triggers")
    };
    let departure = options
        .iter()
        .find(|option| option.line == 2)
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(departure))
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Pending(1))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["incoming"]),
        "the holder already left"
    );
    assert_eq!(
        state["P1"]["hand"],
        json!(["drawn"]),
        "the older matched entry fact remains authoritative"
    );
}

#[test]
fn entry_method_equality_admission_does_not_open_other_equality_domains() {
    let mut candidate = program("ability", ZoneKind::Ex, false).card().clone();
    let Node::Control {
        control: Control::If { condition, .. },
    } = &mut candidate.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("the candidate has one conditional body");
    };
    *condition = Expr::Compute(Box::new(Predicate::Equal {
        operands: Box::new(ValueEquality::Bool {
            left: Expr::Literal(Bool(true)),
            right: Expr::Literal(Bool(true)),
        }),
    }));
    let checked = WellFormedCardProgram::check(candidate).unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "entry-domain",
    )
    .unwrap();
    let error = game
        .load_internal_card("unit-follower", &checked)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("shape.ValueEquality.Bool"),
        "EntryMethod equality admission stays closed to other typed equality domains"
    );
}

#[test]
fn watching_another_entry_does_not_supply_the_holders_entry_method() {
    let mut candidate = candidate("ability");
    let entry = &mut candidate["cards"]["unit-follower"]["abilities"][0];
    let mut body = entry.as_object_mut().unwrap().remove("fanfare").unwrap();
    body["on"] = json!({"enter_field":{"who":"self","sel":{"kind":"follower"}}});
    entry["auto"] = body;
    lowerer().lower(&candidate).unwrap_err();
}
