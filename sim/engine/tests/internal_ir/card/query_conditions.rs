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
fn posture_filter_does_not_authorize_unscoped_holder_state() {
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
    let mut game = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "unscoped-posture",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &program)
        .unwrap_err();
}
