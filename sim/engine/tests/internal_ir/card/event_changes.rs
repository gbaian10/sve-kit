use super::*;
use sve_engine::game::internal::InputDomain;

#[test]
fn canonical_stat_header_checks_local_binding_collisions() {
    for duplicate in [false, true] {
        let mut observer = ability(1, vec![]);
        let scope = observer.node.scope;
        let event = Binding::new(900, scope);
        let subject = Binding::new(if duplicate { 900 } else { 901 }, scope);
        let facet = Binding::new(902, scope);
        observer.node.kind = AbilityKind::Triggered;
        observer.node.play = None;
        observer.node.trigger = Some(TriggerSpec {
            on: TriggerCondition::Event {
                pattern: EventPattern::StatChange {
                    binding: event,
                    predicate: Expr::Compute(Box::new(Predicate::HolderIsObject {
                        holder: Expr::Frozen(Box::new(HolderFields::StatChangeSubject { event })),
                        object: subject,
                        predicate: Expr::Compute(Box::new(Predicate::And {
                            predicates: vec![Expr::Compute(Box::new(Predicate::Or {
                                predicates: [Stat::Attack, Stat::Life]
                                    .into_iter()
                                    .map(|property| {
                                        Expr::Compute(Box::new(Predicate::AnyStatFacet {
                                            facets: Expr::Frozen(Box::new(
                                                FacetFields::StatChangeFacets { event },
                                            )),
                                            facet,
                                            property,
                                            predicate: Expr::Literal(Bool(true)),
                                        }))
                                    })
                                    .collect(),
                            }))],
                        })),
                    })),
                },
            },
            active_zones: vec![ZoneKind::Field],
            view: EventSnapshotPolicy::AfterEntry,
            limit: None,
            aggregation: TriggerAggregation::PerFact {},
        });
        let checked = WellFormedCardProgram::check(CardProgram {
            abilities: vec![observer],
            ..card(vec![]).card().clone()
        });
        assert_eq!(checked.is_ok(), !duplicate);
    }
}

fn event_tree(paid: bool) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut observer = tree["cards"]["BP01-041"].clone();
    let mut trigger = observer["abilities"][0].clone();
    trigger["line"] = json!(1_u8);
    trigger.as_object_mut().unwrap().remove("activate");
    let mut on = json!({});
    on[if paid { "acted" } else { "stood" }] = json!({"ref":"this"});
    trigger["auto"] = json!({"on":on,"do":[{"life":{"who":"self","delta":1}}]});
    let mut activation = observer["abilities"][0].clone();
    activation["line"] = json!(2_u8);
    activation["activate"] =
        json!({"cost":{"act_self":true},"do":[{"life":{"who":"self","delta":0}}]});
    observer["abilities"] = if paid {
        json!([trigger, activation])
    } else {
        json!([trigger])
    };
    let mut producer = tree["cards"]["BP01-041"].clone();
    producer["abilities"][0]["activate"] = json!({"cost":{"pp":1},"do":[{"stand":{"ref":{"who":"self","zone":"field","kind":"follower"}}}]});
    tree["cards"] = json!({"unit-follower":observer,"unit-amulet":producer});
    tree
}

#[test]
fn standing_and_acting_as_a_cost_emit_the_same_ordinary_pending_flow() {
    for paid in [false, true] {
        let tree = event_tree(paid);
        let programs = lowerer().lower(&tree).unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"observer","card":"unit-follower","state":{"acted":!paid}},
            {"id":"producer","card":"unit-amulet"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([]);
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P2"]["zones"]["hand"] = json!([]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "posture-cost-and-stand",
        )
        .unwrap();
        for number in ["unit-follower", "unit-amulet"] {
            game.load_internal_card(number, &programs[number]).unwrap();
        }
        let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
            .as_u64()
            .unwrap();
        let (holder, program, index) = if paid {
            ("observer", "unit-follower", 1)
        } else {
            ("producer", "unit-amulet", 0)
        };
        resolved(
            game.activate_internal_card(
                holder,
                programs[program].card().abilities[index].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("one committed posture event")
        };
        assert_eq!(options.len(), 1);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            before + 1
        );
        assert!(game.internal_pending_choice().unwrap().is_none());
    }
}

#[test]
fn rule_start_standing_finishes_before_main_phase_arrives() {
    let mut tree = event_tree(false);
    let mut main = tree["cards"]["unit-follower"]["abilities"][0].clone();
    main["line"] = json!(2_u8);
    main["auto"] = json!({"on":{"phase":{"whose":"self","which":"main"}},"do":[{"life":{"who":"self","delta":2}}]});
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(main);
    let programs = lowerer().lower(&tree).unwrap();
    let mut initial = setup(1, json!([]));
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"observer","card":"unit-follower","state":{"acted":true}},
        {"id":"producer","card":"unit-amulet"}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    initial["players"]["P2"]["zones"]["hand"] = json!([]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "rule-stand-before-main",
    )
    .unwrap();
    for number in ["unit-follower", "unit-amulet"] {
        game.load_internal_card(number, &programs[number]).unwrap();
    }
    let before = game.projection(View::Referee).unwrap()["P1"]["leader"]["life"]
        .as_u64()
        .unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick::pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["turn"]["phase"],
        "start"
    );
    for line in [1_u64, 2] {
        let question = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &question.domain else {
            panic!("rule stand followed by Main")
        };
        assert_eq!(
            options.len(),
            1,
            "Main is not collected until the stand occurrence finishes"
        );
        assert_eq!(options[0].line, line);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap(),
        );
        game.resume_internal_card().unwrap();
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["phase"], "main");
    assert_eq!(state["P1"]["leader"]["life"], before + 3);
    assert!(game.internal_pending_choice().unwrap().is_none());
}
