use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

fn tree(condition: Value, targets: Value) -> Value {
    let has_targets = !targets.as_array().unwrap().is_empty();
    let mut tree = fanfare::expanded::spell_tree().clone();
    let mut entry = tree["cards"]["SD03-005"].take();
    entry["abilities"][0]["spell"] = json!({"do":[
        {"if":{"then":[{"damage":{"to":{"leader":"opp"},"n":4}}],
            "else":[{"damage":{"to":{"leader":"opp"},"n":3}}]}}
    ]});
    entry["abilities"][0]["spell"]["do"][0]["if"]["cond"] = condition;
    if has_targets {
        entry["abilities"][0]["spell"]["targets"] = targets;
    }
    tree["cards"] = json!({"unit-spell":entry});
    tree
}

fn fixture_catalog() -> Arc<Catalog> {
    let (snapshot, keywords, document) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" {
                row["faces"][0]["traits"] = json!(["竜族", "戦士"]);
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    )
}

#[test]
fn selected_conditions_read_only_the_selected_objects_current_characteristics() {
    for (filter, matches) in [
        (json!({"trait":"竜族"}), true),
        (json!({"trait":"信仰"}), false),
        (json!({"traits_all":["竜族","戦士"]}), true),
        (json!({"traits_all":["竜族","信仰"]}), false),
        (json!({"name":"unit-follower"}), true),
        (json!({"name_contains":"follower"}), true),
        (json!({"name":"unit-amulet"}), false),
        (json!({"class":"ニュートラル"}), true),
        (json!({"class":"ドラゴン"}), false),
        (json!({"not_class":"ドラゴン"}), true),
        (json!({"kind":"evolve_follower"}), false),
        (json!({"acted":true}), false),
        (json!({"acted":false}), true),
    ] {
        let mut query = filter;
        query["in"] = json!("picked");
        let programs = lowerer().lower_with_printed_costs(&tree(json!({"exists":query}),json!([
            {"bind":"picked","from":{"who":"opp","zone":"field","kind":"follower"},"upto":1}
        ])), &BTreeMap::from([("unit-spell".to_owned(),Nat(1))])).unwrap();
        for pick in [false, true] {
            let mut game = Game::new(
                fixture_catalog(),
                &setup(1, json!([])),
                &Value::Null,
                &Value::Null,
                "selected-filter",
            )
            .unwrap();
            game.load_internal_card("unit-spell", &programs["unit-spell"])
                .unwrap();
            game.load_internal_card("unit-follower", &card(vec![]))
                .unwrap();
            let CardPlay::Awaiting(q) = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap()
            else {
                panic!("target")
            };
            let InputDomain::Targets(domain) = &q.domain else {
                panic!("targets")
            };
            let chosen = if pick {
                vec![domain.members[0].handle]
            } else {
                vec![]
            };
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Targets(chosen))
                    .unwrap(),
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                if pick && matches { 16 } else { 17 }
            );
        }
    }
}

#[test]
fn public_existence_filters_cost_life_name_and_controller() {
    for (filter, matches) in [
        (json!({"orig_cost_eq":1}), true),
        (json!({"orig_cost_ge":2}), false),
        (json!({"orig_cost_le":1}), true),
        (json!({"cost_eq":1,"cost_ge":1,"cost_le":1}), true),
        (json!({"hp_le":3}), true),
        (json!({"hp_le":2}), false),
        (json!({"name_contains":"follower"}), true),
        (json!({"name_contains":"amulet"}), false),
    ] {
        let mut query = filter;
        query["who"] = json!("self");
        query["zone"] = json!("field");
        query["kind"] = json!("follower");
        let programs = lowerer()
            .lower_with_printed_costs(
                &tree(json!({"exists":query}), json!([])),
                &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
            )
            .unwrap();
        for present in [false, true] {
            let mut initial = setup(1, json!([]));
            if present {
                initial["players"]["P1"]["zones"]["field"] =
                    json!([{"id":"own","card":"unit-follower"}]);
            }
            let mut game = Game::new(
                catalog(),
                &initial,
                &Value::Null,
                &Value::Null,
                "exists-filter",
            )
            .unwrap();
            game.load_internal_card("unit-spell", &programs["unit-spell"])
                .unwrap();
            game.load_internal_card("unit-follower", &card(vec![]))
                .unwrap();
            resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
            assert_eq!(
                game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
                if present && matches { 16 } else { 17 }
            );
        }
    }
}

#[test]
fn selected_conditions_reject_unbound_names_observations_and_foreign_zone_fields() {
    for query in [
        json!({"in":"unknown","trait":"竜族"}),
        json!({"in":"picked","who":"opp","trait":"竜族"}),
        json!({"in":"picked","zone":"hand","trait":"竜族"}),
        json!({"who":"opp","zone":"hand","name_contains":"unit"}),
    ] {
        lowerer()
            .lower_with_printed_costs(
                &tree(json!({"exists":query}), json!([])),
                &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
            )
            .unwrap_err();
    }
}

#[test]
fn selected_condition_does_not_follow_a_target_into_hidden_hand_generation() {
    let mut candidate = tree(
        json!({"exists":{"in":"picked","name":"unit-follower"}}),
        json!([
            {"bind":"picked","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}
        ]),
    );
    candidate["cards"]["unit-spell"]["abilities"][0]["spell"]["do"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"move":{"ref":"picked","to":"hand"}}));
    let programs = lowerer()
        .lower_with_printed_costs(
            &candidate,
            &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]),
        )
        .unwrap();
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "departed-selected",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &programs["unit-spell"])
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    let CardPlay::Awaiting(q) = game.play_internal_card("s", PlayerId { seat: 1 }).unwrap() else {
        panic!("target")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("targets")
    };
    resolved(
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["hand"], json!(["f"]));
    assert_eq!(state["P2"]["leader"]["life"], 17);
    assert!(
        game.projection(View::P1).unwrap()["objects"]
            .get("f")
            .is_none()
    );
}

#[test]
fn selected_conditions_do_not_authorize_a_private_look_choice() {
    let mut candidate = tree(
        json!({"exists":{"in":"picked","name":"unit-follower"}}),
        json!([]),
    );
    let steps = candidate["cards"]["unit-spell"]["abilities"][0]["spell"]["do"]
        .as_array_mut()
        .unwrap();
    steps.insert(
        0,
        json!({"look":{"top":{"who":"self","n":1},"bind":"seen"}}),
    );
    steps.insert(
        1,
        json!({"choose":{"bind":"picked","from":{"in":"seen","kind":"follower"},"n":1,"may":true}}),
    );
    let costs = BTreeMap::from([("unit-spell".to_owned(), Nat(1))]);
    let mut allowed = candidate.clone();
    allowed["cards"]["unit-spell"]["abilities"][0]["spell"]["do"][2]["if"]["cond"] =
        json!({"cmp":{"left":1,"op":"eq","right":1}});
    lowerer()
        .lower_with_printed_costs(&allowed, &costs)
        .unwrap();
    assert_eq!(
        lowerer()
            .lower_with_printed_costs(&candidate, &costs)
            .err()
            .unwrap()
            .code,
        "unsupported_construct"
    );
}

#[test]
fn source_posture_condition_is_bound_to_the_current_ability_holder() {
    let condition = Expr::Compute(Box::new(Predicate::PostureEquals {
        left: Expr::Live(Box::new(PostureRead::Posture {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
        })),
        right: Expr::Literal(Posture::Standing),
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
    for acted in [false, true] {
        let mut game = position_with_state(1, &program, &json!({"acted":acted}));
        activate(&mut game, 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if acted { json!([]) } else { json!(["d"]) }
        );
        assert_eq!(
            state["P1"]["deck"],
            if acted { json!(["d"]) } else { json!([]) }
        );
    }
}

#[test]
fn choose_a_subset_of_an_earlier_public_binding_keeps_the_original_population() {
    for by in ["self", "opp"] {
        let document = equipment_played::tree(
            &json!({"activate":{
                "cost":{"pp":0},"targets":[{"bind":"population","from":{"who":"opp","zone":"field","kind":"follower"},"n":2}],
                "do":[{"choose":{"bind":"subset","by":by,"from":{"in":"population"},"upto":1}},{"act":"subset"}]
            }}),
            "unit-amulet",
        );
        let program = lowerer()
            .lower(&document)
            .unwrap()
            .remove("unit-amulet")
            .unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-amulet"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"a","card":"unit-follower"},{"id":"b","card":"unit-follower"},{"id":"outside","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "bound-subset",
        )
        .unwrap();
        game.load_internal_card("unit-amulet", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        let CardPlay::Awaiting(formal) = game
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("formal population")
        };
        let CardPlay::Awaiting(subset) = game
            .answer_internal_card(formal.actor, formal.token, Answer::Targets(vec![0, 1]))
            .unwrap()
        else {
            panic!("public subset")
        };
        let InputDomain::Targets(domain) = &subset.domain else {
            panic!("subset objects")
        };
        assert_eq!(subset.actor.seat, if by == "self" { 1 } else { 2 });
        assert_eq!(domain.members.len(), 2);
        resolved(
            game.answer_internal_card(subset.actor, subset.token, Answer::Targets(vec![1]))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["a"]["acted"], false);
        assert_eq!(state["objects"]["b"]["acted"], true);
        assert_eq!(state["objects"]["outside"]["acted"], false);
    }
}

#[test]
fn keyword_filtered_buff_uses_effective_abilities_and_the_controller() {
    let spell=lowerer().lower_with_printed_costs(&equipment_played::tree(&json!({"spell":{"do":[
        {"buff":{"on":{"who":"self","zone":"field","kind":"follower","keyword":"ward"},"power":1,"hp":2}}
    ]}}),"unit-spell"),&BTreeMap::from([("unit-spell".to_owned(),Nat(1))])).unwrap().remove("unit-spell").unwrap();
    let ward = lowerer()
        .lower(&equipment_played::tree(
            &json!({"static":{"effects":[{"grant":{"keywords":["ward"]}}]}}),
            "unit-follower",
        ))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"ward","card":"unit-follower"},
        {"id":"muted","card":"unit-follower","state":{"silenced":true}}
    ]);
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "keyword-buff",
    )
    .unwrap();
    game.load_internal_card("unit-spell", &spell).unwrap();
    game.load_internal_card("unit-follower", &ward).unwrap();
    resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ward"]["power"], 3_u8);
    assert_eq!(state["objects"]["ward"]["hp"], 5_u8);
    for object in ["muted", "f"] {
        assert_eq!(state["objects"][object]["power"], 2_u8);
        assert_eq!(state["objects"][object]["hp"], 3_u8);
    }
}

#[test]
fn keyword_filtered_numeric_changes_exclude_their_own_holder() {
    let mut document = equipment_played::tree(
        &json!({"static":{"effects":[{"grant":{"keywords":["ward"]}}]}}),
        "unit-follower",
    );
    let mut activation = document["cards"]["unit-follower"]["abilities"][0].clone();
    activation.as_object_mut().unwrap().remove("static");
    activation["sources"][0]["ability_scope"] = json!("buff-other-ward");
    activation["activate"] = json!({"cost":{"pp":0},"do":[{"buff":{
        "on":{"who":"self","zone":"field","kind":"follower","keyword":"ward","other":true},"power":1,"hp":2
    }}]});
    document["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(activation);
    let program = lowerer()
        .lower(&document)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"unit-follower"},{"id":"other","card":"unit-follower"}]);
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "other-ward-buff",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program).unwrap();
    resolved(
        game.activate_internal_card(
            "source",
            program.card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
    assert_eq!(state["objects"]["source"]["hp"], 3_u8);
    assert_eq!(state["objects"]["other"]["power"], 3_u8);
    assert_eq!(state["objects"]["other"]["hp"], 5_u8);
    assert_eq!(state["objects"]["f"]["power"], 2_u8);
}

#[test]
fn numeric_keyword_filters_keep_the_existing_field_and_ex_zone_boundary() {
    for kind in ["buff", "set_stat"] {
        let mut step = json!({});
        step[kind] = json!({"on":{"who":"self","zone":"field","keyword":"ward"},"power":1});
        let mut document = equipment_played::tree(
            &json!({"activate":{"cost":{"pp":0},"do":[step]}}),
            "unit-follower",
        );
        lowerer().lower(&document).unwrap();
        document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0][kind]["on"]["zone"] =
            json!("cemetery");
        lowerer().lower(&document).unwrap_err();
    }
}
