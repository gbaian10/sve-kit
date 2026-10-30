use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

fn tree(steps: &Value) -> Value {
    let mut tree = fanfare::expanded::spell_tree().clone();
    tree["cards"]["SD03-005"]["abilities"][0]["spell"] = json!({"do":steps});
    tree
}

fn move_success_tree(steps: &Value) -> Value {
    let mut tree = tree(steps);
    let mut candidate = tree["cards"]["SD03-005"].take();
    candidate["abilities"][0]["spell"]["targets"] = json!([
        {"bind":"t","from":{"who":"self","zone":"field","kind":"follower"},"upto":2}
    ]);
    tree["cards"] = json!({"unit-spell":candidate});
    tree
}

fn move_success_steps() -> Value {
    json!([
        {"move":{"ref":"t","to":"ex","result":"moved"}},
        {"if":{"cond":{"did":"moved"},"then":[{"life":{"who":"self","delta":2}}]}}
    ])
}

#[test]
fn move_success_condition_uses_successful_members_instead_of_selected_targets() {
    let programs = lowerer()
        .lower_with_printed_costs(
            &move_success_tree(&move_success_steps()),
            &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
        )
        .unwrap();
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        for (selected, occupied, healed) in
            [(0, 0, false), (1, 0, true), (2, 0, true), (1, 5, false)]
        {
            let mut initial = setup(seat, json!([]));
            initial["turn"]["active"] = json!(actor);
            initial["players"][actor]["leader"]["life"] = json!(10);
            initial["players"][actor]["zones"]["field"] = json!([
                {"id":"one","card":"unit-follower"},{"id":"two","card":"unit-follower"}
            ]);
            initial["players"][actor]["zones"]["ex"] = json!(
                (0..occupied)
                    .map(|n| json!({"id":format!("ex{n}"),"card":"unit-follower"}))
                    .collect::<Vec<_>>()
            );
            let mut game = Game::new(
                catalog(),
                &initial,
                &Value::Null,
                &Value::Null,
                "move-success",
            )
            .unwrap();
            game.load_internal_card("unit-spell", &programs["unit-spell"])
                .unwrap();
            game.load_internal_card("unit-follower", &card(vec![]))
                .unwrap();
            let CardPlay::Awaiting(question) =
                game.play_internal_card("s", PlayerId { seat }).unwrap()
            else {
                panic!("the target group is declared before movement");
            };
            let InputDomain::Targets(domain) = &question.domain else {
                panic!("targets");
            };
            let handles = domain
                .members
                .iter()
                .take(selected)
                .map(|m| m.handle)
                .collect();
            let result = resolved(
                game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
                    .unwrap(),
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after[actor]["leader"]["life"], if healed { 12 } else { 10 });
            assert_eq!(
                after[actor]["ex"].as_array().unwrap().len(),
                occupied + if healed { selected } else { 0 }
            );
            assert_eq!(
                after[actor]["field"].as_array().unwrap().len(),
                2 - if healed { selected } else { 0 }
            );
            assert_eq!(
                result
                    .progress
                    .facts
                    .iter()
                    .filter(|f| matches!(f, Fact::Movement(_)))
                    .count(),
                if healed { selected } else { 0 }
            );
        }
    }
}

#[test]
fn move_success_condition_rejects_missing_wrong_kind_filtered_and_forward_receipts() {
    for steps in [
        json!([{"if":{"cond":{"did":"missing"},"then":[{"life":{"delta":2}}]}}]),
        json!([{"if":{"cond":{"did":"moved"},"then":[{"life":{"delta":2}}]}},{"move":{"ref":"t","to":"ex","result":"moved"}}]),
        json!([{"move":{"ref":"t","to":"ex","result":"moved"}},{"if":{"cond":{"did":{"result":"moved","where":{"kind":"follower"}}},"then":[{"life":{"delta":2}}]}}]),
        json!([{"move":{"ref":"t","to":"ex","result":"t"}},{"if":{"cond":{"did":"t"},"then":[{"life":{"delta":2}}]}}]),
        json!([{"if":{"cond":{"awakened":true},"then":[{"move":{"ref":"t","to":"ex","result":"moved"}}]}},{"if":{"cond":{"did":"moved"},"then":[{"life":{"delta":2}}]}}]),
    ] {
        lowerer()
            .lower_with_printed_costs(
                &move_success_tree(&steps),
                &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
            )
            .unwrap_err();
    }
    let mut observed = move_success_tree(&json!([
        {"look":{"top":{"who":"self","n":1},"bind":"looked"}},
        {"if":{"cond":{"did":"looked"},"then":[{"life":{"delta":2}}]}}
    ]));
    observed["cards"]["unit-spell"]["abilities"][0]["spell"]
        .as_object_mut()
        .unwrap()
        .remove("targets");
    lowerer()
        .lower_with_printed_costs(
            &observed,
            &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
        )
        .unwrap_err();
}

#[test]
fn move_success_condition_accepts_one_success_in_a_mixed_owner_batch() {
    let programs = lowerer()
        .lower_with_printed_costs(
            &move_success_tree(&move_success_steps()),
            &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
        )
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["leader"]["life"] = json!(10);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"blocked","card":"unit-follower"},
        {"id":"moved","card":"unit-follower","owner":"P2"}
    ]);
    initial["players"]["P1"]["zones"]["ex"] = json!(
        (0..5)
            .map(|n| json!({"id":format!("ex{n}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "partial-move-success",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &programs["unit-spell"])
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    let CardPlay::Awaiting(question) = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
    else {
        panic!("targets")
    };
    resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 1]))
            .unwrap(),
    );
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["leader"]["life"], 12);
    assert_eq!(after["P1"]["field"], json!(["blocked"]));
    assert_eq!(after["P1"]["ex"].as_array().unwrap().len(), 5);
    assert_eq!(after["P2"]["ex"], json!(["moved"]));
}

#[test]
fn boolean_conditions_use_current_pp_and_unfiltered_hidden_counts_without_exposing_cards() {
    let programs = lowerer().lower_with_printed_costs(
        &tree(&json!([{"if":{
            "cond":{"all":[
                {"cmp":{"left":{"resource":{"who":"self","kind":"pp"}},"op":"eq","right":2}},
                {"cmp":{"left":{"count":{"who":"self","zone":"deck"}},"op":"ge","right":1}},
                {"any":[
                    {"not":{"cmp":{"left":{"count":{"who":"opp","zone":"hand"}},"op":"eq","right":0}}},
                    {"cmp":{"left":1,"op":"gt","right":2}}
                ]}
            ]},
            "then":[{"draw":{"n":1}}],
            "else":[{"damage":{"to":{"leaders":"all"},"n":1}}]
        }}])),
        &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
    ).unwrap();
    for hidden in [false, true] {
        let (catalog, mut initial) =
            fanfare::fragment_setup(1, "SD03-005", 3, json!([{"id":"drawn","card":"BP01-037"}]));
        initial["players"]["P2"]["zones"]["hand"] = if hidden {
            json!([{"id":"secret","card":"BP01-037"}])
        } else {
            json!([])
        };
        let mut game = Game::new(
            catalog,
            &initial,
            &Value::Null,
            &Value::Null,
            "body-condition",
        )
        .unwrap();
        game.load_internal_card("SD03-005", &programs["SD03-005"])
            .unwrap();
        game.load_internal_card("BP01-037", &card(vec![])).unwrap();
        resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if hidden { json!(["drawn"]) } else { json!([]) }
        );
        assert_eq!(state["P2"]["leader"]["life"], if hidden { 20 } else { 19 });
        let public = game.projection(View::P1).unwrap();
        assert!(public["objects"].get("secret").is_none());
    }
}

#[test]
fn conditions_reject_filtered_hidden_counts_and_non_numeric_damage_replacements() {
    for steps in [
        json!([{"if":{"cond":{"cmp":{"left":{"count":{"who":"opp","zone":"hand","kind":"follower"}},"op":"ge","right":1}},"then":[{"draw":{"n":1}}]}}]),
        json!([{"draw":{"n":{"count":{"who":"self","zone":"hand","not_token":true}}}}]),
        json!([{"draw":{"n":{"count":{"who":"self","zone":"deck","token":true}}}}]),
        json!([{"damage":{"to":{"leaders":"all"},"n":1,"instead":[{"if":{"cmp":{"left":1,"op":"eq","right":1}},"to":{"leader":"self"}}]}}]),
        json!([{"damage":{"to":{"leaders":"all"},"n":1,"instead":[
            {"if":{"cmp":{"left":1,"op":"eq","right":1}},"n":2},
            {"if":{"cmp":{"left":1,"op":"eq","right":1}},"n":3}
        ]}}]),
    ] {
        lowerer()
            .lower_with_printed_costs(
                &tree(&steps),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap_err();
    }
}

#[test]
fn count_admission_does_not_authorize_hand_objects_as_effect_targets() {
    let hidden = Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        },
        zone: ZoneKind::Hand,
        view: SelectionView::Current {},
        binding: Binding::new(99, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    };
    let program = card(vec![ability(
        1,
        vec![instruction(1, Action::Destroy { objects: hidden })],
    )]);
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "hidden-selector",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
}

#[test]
fn leader_life_threshold_and_expression_read_after_the_preceding_life_change() {
    for (who, life) in [("self", 9_i64), ("opp", 20_i64)] {
        let programs = lowerer()
            .lower_with_printed_costs(
                &tree(&json!([
                    {"life":{"who":"self","delta":-11}},
                    {"if":{"cond":{"all":[
                        {"leader_life":{"who":who,"eq":life}},
                        {"leader_life":{"who":who,"le":life}},
                        {"leader_life":{"who":who,"ge":life}},
                        {"cmp":{"left":{"leader_life":who},"op":"eq","right":life}}
                    ]},"then":[{"draw":{"n":1}}]}}
                ])),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap();
        let (catalog, initial) =
            fanfare::fragment_setup(1, "SD03-005", 3, json!([{"id":"drawn","card":"BP01-037"}]));
        let mut game =
            Game::new(catalog, &initial, &Value::Null, &Value::Null, "live-life").unwrap();
        game.load_internal_card("SD03-005", &programs["SD03-005"])
            .unwrap();
        game.load_internal_card("BP01-037", &card(vec![])).unwrap();
        resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 9_i64);
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    }
}

#[test]
fn live_condition_syntax_rejects_false_awakening_and_ambiguous_thresholds() {
    for condition in [
        json!({"awakened":false}),
        json!({"leader_life":{"who":"self"}}),
        json!({"leader_life":{"who":"self","le":10,"ge":5}}),
        json!({"leader_life":{"who":"all","eq":10}}),
    ] {
        lowerer()
            .lower_with_printed_costs(
                &tree(&json!([{"if":{"cond":condition,"then":[{"draw":{"n":1}}]}}])),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap_err();
    }
}

#[test]
fn crimson_false_is_rejected_instead_of_becoming_a_negated_history_condition() {
    lowerer()
        .lower_with_printed_costs(
            &tree(&json!([{"if":{"cond":{"crimson":false},"then":[{"draw":{"n":1}}]}}])),
            &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
        )
        .unwrap_err();
}

#[test]
fn named_existence_admission_keeps_hidden_names_private() {
    for zone_kind in [ZoneKind::Field, ZoneKind::Hand] {
        let binding = Binding::new(99, ScopeId(2));
        let condition = Expr::Compute(Box::new(Predicate::Exists {
            objects: Selector::Zone {
                players: PlayerSet {
                    relation: PlayerRelation::Opponents,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(0),
                        maximum: None,
                    },
                    text_quantifier: TextQuantifier::Implicit,
                },
                zone: zone_kind,
                view: SelectionView::Current {},
                binding,
                predicate: Box::new(Expr::Compute(Box::new(Predicate::NamesIntersect {
                    left: Expr::Live(Box::new(NamesRead::Names {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::Bound { binding }),
                        },
                    })),
                    right: Expr::Literal(vec![CardName("unit-follower".into())]),
                }))),
            },
        }));
        let program = card(vec![ability(
            1,
            vec![node(Node::Control {
                control: Control::If {
                    condition,
                    then_body: Program {
                        scope: ScopeId(3),
                        steps: vec![draw(1, 1)],
                    },
                    else_body: None,
                },
            })],
        )]);
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "named-existence-admission",
        )
        .unwrap();
        let accepted = game.load_internal_card("unit-follower", &program).is_ok();
        assert_eq!(accepted, zone_kind == ZoneKind::Field);
    }
}

#[test]
fn name_contains_outside_a_candidate_selector_is_not_admitted() {
    let condition = Expr::Compute(Box::new(Predicate::NameContains {
        object: ObjectView::Current {
            object: Box::new(ObjectOperand::SourceHolder {}),
        },
        name: Expr::Literal(CardName("unit".into())),
    }));
    let program = card(vec![ability(
        1,
        vec![node(Node::Control {
            control: Control::If {
                condition,
                then_body: Program {
                    scope: ScopeId(3),
                    steps: vec![draw(1, 1)],
                },
                else_body: None,
            },
        })],
    )]);
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "unsupported-name-location",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
}

#[test]
fn public_exists_condition_accepts_public_zones_and_other_followers() {
    for query in [
        json!({"who":"self","zone":"field","kind":"follower","trait":"魔法使い"}),
        json!({"who":"self","zone":"field","kind":"follower","trait":"魔法使い","other":true}),
        json!({"who":"self","zone":"ex","kind":"follower","trait":"魔法使い","other":true}),
        json!({"who":"opp","zone":"cemetery","kind":"follower","other":true}),
    ] {
        lowerer()
            .lower_with_printed_costs(
                &tree(&json!([{"if":{
                    "cond":{"exists":query},
                    "then":[{"draw":{"n":1}}]
                }}])),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap();
    }
}

#[test]
fn public_exists_condition_rejects_private_zones_and_unsupported_filters() {
    for query in [
        json!({"who":"self","zone":"hand","kind":"follower","other":true}),
        json!({"who":"self","zone":"field","not_trait":"魔法使い","other":true}),
        json!({"who":"self","zone":"field","hp_le":3,"other":true}),
    ] {
        lowerer()
            .lower_with_printed_costs(
                &tree(&json!([{"if":{
                    "cond":{"exists":query},"then":[{"draw":{"n":1}}]
                }}])),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap_err();
    }
}

#[test]
fn other_field_condition_accepts_admitted_class_and_token_filters() {
    for query in [
        json!({"who":"self","zone":"field","class":"ウィッチ","other":true}),
        json!({"who":"self","zone":"field","not_class":"ウィッチ","other":true}),
        json!({"who":"self","zone":"field","trait":"魔法使い","token":true,"other":true}),
    ] {
        lowerer()
            .lower_with_printed_costs(
                &tree(&json!([{"if":{
                    "cond":{"exists":query},"then":[{"draw":{"n":1}}]
                }}])),
                &BTreeMap::from([("SD03-005".to_owned(), Nat(1))]),
            )
            .unwrap();
    }
}

#[test]
fn filtered_public_count_numbers_drive_draw_life_damage_and_stats() {
    for zone in ["cemetery", "ex"] {
        let count =
            json!({"count":{"who":"self","zone":zone,"kind":"follower","name_contains":"unit-"}});
        let mut candidate = move_success_tree(&json!([
            {"life":{"who":"self","delta":count}},
            {"draw":{"n":count}},
            {"damage":{"to":{"leader":"opp"},"n":count}},
            {"buff":{"on":{"who":"opp","zone":"field","kind":"follower"},"power":count}}
        ]));
        candidate["cards"]["unit-spell"]["abilities"][0]["spell"]
            .as_object_mut()
            .unwrap()
            .remove("targets");
        let programs = lowerer()
            .lower_with_printed_costs(
                &candidate,
                &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
            )
            .unwrap();
        let mut initial = setup(
            1,
            json!([{"id":"d1","card":"unit-follower"},{"id":"d2","card":"unit-follower"},{"id":"d3","card":"unit-follower"}]),
        );
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"][zone] = json!([
            {"id":"g1","card":"unit-follower"},{"id":"g2","card":"unit-follower"},{"id":"nonmatch","card":"unit-amulet"}
        ]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "filtered-numbers",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &programs["unit-spell"])
            .unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["leader"]["life"], 12_u8);
        assert_eq!(state["P2"]["leader"]["life"], 18_u8);
        assert_eq!(state["P1"]["hand"], json!(["d1", "d2"]));
        assert_eq!(state["objects"]["f"]["power"], 4_u8);
    }
}

#[test]
fn public_token_counts_filter_identity_owner_and_zone() {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-amulet" {
                row["faces"][0]["card_type"] = json!("フォロワー・トークン");
                row["faces"][0]["power"] = json!("2");
                row["faces"][0]["hp"] = json!("3");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    );
    for zone in ["field", "ex"] {
        for token in [false, true] {
            let mut count = json!({"count":{"who":"self","zone":zone,"kind":"follower"}});
            count["count"][if token { "token" } else { "not_token" }] = json!(true);
            let mut candidate = move_success_tree(&json!([{"life":{"who":"self","delta":count}}]));
            candidate["cards"]["unit-spell"]["abilities"][0]["spell"]
                .as_object_mut()
                .unwrap()
                .remove("targets");
            let programs = lowerer()
                .lower_with_printed_costs(
                    &candidate,
                    &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
                )
                .unwrap();
            let mut initial = setup(1, json!([]));
            initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
            initial["players"]["P1"]["zones"][zone] = json!([
                {"id":"token-one","card":"unit-amulet"},
                {"id":"token-two","card":"unit-amulet"},
                {"id":"ordinary","card":"unit-follower"}
            ]);
            let other_zone = if zone == "field" { "ex" } else { "field" };
            initial["players"]["P1"]["zones"][other_zone] =
                json!([{"id":"other-zone","card":"unit-amulet"}]);
            initial["players"]["P2"]["zones"][zone] = json!([
                {"id":"opponent-one","card":"unit-amulet"},
                {"id":"opponent-two","card":"unit-amulet"}
            ]);
            let mut game = Game::new(
                Arc::clone(&catalog),
                &initial,
                &Value::Null,
                &Value::Null,
                "public-token-count",
            )
            .unwrap();
            game.load_internal_card("unit-spell", &programs["unit-spell"])
                .unwrap();
            for number in ["unit-follower", "unit-amulet"] {
                game.load_internal_card(number, &card(vec![])).unwrap();
            }
            resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
                if token { 12_u8 } else { 11_u8 }
            );
        }
    }
}

#[test]
fn distinct_counts_never_authorize_hidden_zone_observations() {
    for zone in [ZoneKind::Hand, ZoneKind::Deck] {
        for property in [ObjectProperty::Names, ObjectProperty::Cost] {
            let amount = Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Compute(Box::new(NatOperation::DistinctCount {
                    objects: Selector::Zone {
                        players: PlayerSet {
                            relation: PlayerRelation::Opponents,
                            relative_to: PlayerRef::Controller {},
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            text_quantifier: TextQuantifier::Implicit,
                        },
                        zone,
                        view: SelectionView::Current {},
                        binding: Binding::new(99, ScopeId(1)),
                        predicate: Box::new(Expr::Literal(Bool(true))),
                    },
                    property,
                })),
            }));
            for conditional in [false, true] {
                let value = if conditional {
                    Expr::Compute(Box::new(IntOperation::Conditional {
                        condition: Expr::Compute(Box::new(Predicate::CompareInt {
                            left: amount.clone(),
                            comparison: Comparison::GreaterEqual,
                            right: Expr::Literal(Int(1)),
                        })),
                        then_value: Expr::Literal(Int(1)),
                        else_value: Expr::Literal(Int(0)),
                    }))
                } else {
                    amount.clone()
                };
                let program = card(vec![ability(
                    1,
                    vec![instruction(
                        1,
                        Action::Life {
                            player: PlayerRef::Controller {},
                            change: NumericChange::Add { amount: value },
                        },
                    )],
                )]);
                let mut game = Game::new(
                    catalog(),
                    &setup(1, json!([])),
                    &Value::Null,
                    &Value::Null,
                    "private-distinct",
                )
                .unwrap();
                game.load_internal_card("unit-follower", &program)
                    .unwrap_err();
            }
        }
    }
}
