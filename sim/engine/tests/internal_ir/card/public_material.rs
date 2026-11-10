use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::{DecisionView, InputDomain};
use sve_engine::ir::lowering::ExpandedLowerer;

#[test]
fn public_material_union_preserves_banishment_and_rejects_unconstrained_transfers() {
    let body = json!({"cost":{"banish":{"sel":{"who":"self","zone":["field","ex"],"name":"tree"},"n":1}},"do":[{"life":{"who":"self","delta":1}}]});
    for zones in [json!(["field", "ex"]), json!(["ex", "field"])] {
        let mut input = body.clone();
        input["cost"]["banish"]["sel"]["zone"] = zones;
        let typed = program("unit-follower", "activate", input);
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "material-union-admission",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &typed).unwrap();
    }
    for zones in [
        json!([]),
        json!(["field"]),
        json!(["field", "field"]),
        json!(["field", "cemetery"]),
        json!(["field", "ex", "cemetery"]),
    ] {
        let mut input = body.clone();
        input["cost"]["banish"]["sel"]["zone"] = zones;
        lowerer()
            .lower(&expanded("unit-follower", "activate", input))
            .unwrap_err();
    }
    for who in ["opp", "all"] {
        let mut input = body.clone();
        input["cost"]["banish"]["sel"]["who"] = json!(who);
        lowerer()
            .lower(&expanded("unit-follower", "activate", input))
            .unwrap_err();
    }
    for operation in ["act", "to_cemetery", "return_to_hand", "to_ex"] {
        let mut input = body.clone();
        input["cost"] = json!({operation: body["cost"]["banish"]});
        lowerer()
            .lower(&expanded("unit-follower", "activate", input))
            .unwrap_err();
    }
    let mut variable = body;
    variable["cost"]["banish"]
        .as_object_mut()
        .unwrap()
        .remove("n");
    variable["cost"]["banish"]["any"] = json!(true);
    lowerer()
        .lower(&expanded("unit-follower", "activate", variable))
        .unwrap_err();
}

#[test]
fn material_union_does_not_admit_union_conditions_counts_or_aggregates() {
    for zone in [json!("field"), json!(["field", "ex"])] {
        let query = json!({"who":"self","zone":zone,"kind":"follower"});
        for step in [
            json!({"if":{"cond":{"exists":query},"then":[{"life":{"who":"self","delta":1}}]}}),
            json!({"draw":{"n":{"count":query}}}),
            json!({"draw":{"n":{"distinct_names":query}}}),
            json!({"draw":{"n":{"agg":{"of":query,"attr":"power","fn":"sum"}}}}),
        ] {
            let input = expanded(
                "unit-follower",
                "activate",
                json!({"cost":{"pp":1},"do":[step]}),
            );
            let result = lowerer().lower(&input);
            assert_eq!(
                result.is_ok(),
                zone.is_string(),
                "only the existing single-zone query is admitted"
            );
        }
    }
}

fn expanded(card: &str, head: &str, body: Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut entry = tree["cards"]["BP08-038"]["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("fanfare");
    entry[head] = body;
    let meta = tree["cards"]["BP08-038"]["meta"].clone();
    tree["cards"] = json!({card:{"meta":meta,"abilities":[entry]}});
    tree
}

fn program(card: &str, head: &str, body: Value) -> WellFormedCardProgram {
    ExpandedLowerer::new()
        .unwrap()
        .lower_with_printed_costs(
            &expanded(card, head, body),
            &BTreeMap::from([(card.to_owned(), Nat(1))]),
        )
        .unwrap()[card]
        .clone()
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture question fails the scenario."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("expected question")
    };
    question
}

#[test]
fn optional_act_materials_filter_class_kind_and_standing_after_targets() {
    let typed = program(
        "unit-spell",
        "spell",
        json!({
            "optional_cost":{"act":{"sel":{"who":"self","zone":"field","kind":"follower","class":"ロイヤル"},"n":1},"result":"paid"},
            "targets":[{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}],
            "do":[{"if":{"cond":{"paid":{"receipt":"paid"}},"then":[{"damage":{"to":"enemy","n":1}}]}}]
        }),
    );
    let (snapshot, keywords, effects) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" {
                row["faces"][0]["card_class"] = json!("ロイヤル");
            }
            if row["number"] == "unit-amulet" {
                row["faces"][0]["card_type"] = json!("フォロワー");
                row["faces"][0]["power"] = json!("2");
                row["faces"][0]["hp"] = json!("3");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), effects)]).unwrap(),
    );
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"royal","card":"unit-follower"},
        {"id":"acted","card":"unit-follower","state":{"acted":true}},
        {"id":"neutral","card":"unit-amulet"}
    ]);
    for accepted in [false, true] {
        let mut game = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "material-class",
        )
        .unwrap();
        game.load_internal_card("unit-spell", &typed).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        let declaration = awaiting(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        let target = awaiting(
            game.answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(accepted),
            )
            .unwrap(),
        );
        assert!(matches!(target.domain, InputDomain::Targets(_)));
        let play = game
            .answer_internal_card(target.actor, target.token, Answer::Targets(vec![0]))
            .unwrap();
        let result = if accepted {
            let material = awaiting(play);
            let InputDomain::CostMaterials(domain) = &material.domain else {
                panic!("material")
            };
            assert_eq!(domain.members.len(), 1);
            resolved(
                game.answer_internal_card(
                    material.actor,
                    material.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap(),
            )
        } else {
            resolved(play)
        };
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["royal"]["acted"], accepted);
        assert_eq!(state["objects"]["acted"]["acted"], true);
        assert_eq!(state["objects"]["neutral"]["acted"], false);
        assert_eq!(state["objects"]["f"]["damage"], u8::from(accepted));
        assert_eq!(state["P1"]["pp"]["current"], 1);
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::CardPlayed { .. }))
        );
    }
}

#[test]
fn trigger_head_requires_five_own_cemetery_cards_or_cancels_without_playing() {
    let typed = program(
        "unit-follower",
        "fanfare",
        json!({
            "cost":{"banish":{"sel":{"who":"self","zone":"cemetery"},"n":5}},
            "do":[{"life":{"who":"self","delta":1}}]
        }),
    );
    for count in [4, 5] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
            (0..count)
                .map(|n| json!({"id":format!("c{n}"),"card":"unit-spell"}))
                .collect(),
        );
        initial["players"]["P2"]["zones"]["cemetery"] =
            json!([{"id":"foreign","card":"unit-spell"}]);
        let mut game = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "cemetery-head",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &typed).unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        assert!(matches!(
            game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
            CardPlay::Entered(_)
        ));
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let declaration = awaiting(
            game.answer_internal_card(pending.actor, pending.token, Answer::Pending(1))
                .unwrap(),
        );
        let result = game
            .answer_internal_card(
                declaration.actor,
                declaration.token,
                Answer::OptionalCost(true),
            )
            .unwrap();
        if count == 4 {
            assert_eq!(result, CardPlay::Cancelled);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 4);
            assert_eq!(state["P1"]["leader"]["life"], 20);
            game.resume_internal_card().unwrap();
            assert!(game.internal_pending_choice().unwrap().is_none());
            continue;
        }
        let question = awaiting(result);
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("materials")
        };
        assert_eq!(domain.members.len(), 5);
        let before = exact_authority(&game);
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(vec![0, 1, 2, 3, 3]),
        )
        .unwrap_err();
        assert_eq!(before, exact_authority(&game));
        let result = resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0, 1, 2, 3, 4]),
            )
            .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["banish"].as_array().unwrap().len(), 5);
        assert_eq!(state["P2"]["cemetery"], json!(["foreign"]));
        assert_eq!(state["P1"]["leader"]["life"], 21);
        let played = result
            .progress
            .facts
            .iter()
            .position(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .unwrap();
        assert_eq!(
            result.progress.facts[..played]
                .iter()
                .filter(|fact| matches!(fact, Fact::Movement(_)))
                .count(),
            5
        );
    }
}

#[test]
fn optional_public_cost_rejects_extra_operations_and_unknown_keys() {
    let valid = json!({"optional_cost":{"act":{"sel":{"who":"self","zone":"field"},"n":1},"result":"paid"},"do":[{"draw":{"n":1}}]});
    let lowerer = ExpandedLowerer::new().unwrap();
    let costs = BTreeMap::from([("unit-spell".to_owned(), Nat(1))]);
    lowerer
        .lower_with_printed_costs(&expanded("unit-spell", "spell", valid.clone()), &costs)
        .unwrap();
    for (extra, value) in [
        (
            "to_cemetery",
            json!({"sel":{"who":"self","zone":"field"},"n":1}),
        ),
        ("unknown", json!(true)),
    ] {
        let mut body = valid.clone();
        body["optional_cost"][extra] = value;
        lowerer
            .lower_with_printed_costs(&expanded("unit-spell", "spell", body), &costs)
            .unwrap_err();
    }
}

#[test]
fn declining_one_head_keeps_the_other_pending_ability_of_the_same_holder() {
    let mut tree = expanded(
        "unit-follower",
        "fanfare",
        json!({"cost":{"pp":0},"do":[{"life":{"who":"self","delta":1}}]}),
    );
    let mut second = tree["cards"]["unit-follower"]["abilities"][0].clone();
    second["line"] = json!(3);
    second["sources"][0]["occurrence"]["line_ordinal"] = json!(2);
    second["fanfare"] = json!({"do":[{"life":{"who":"self","delta":2}}]});
    tree["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(second);
    let typed = ExpandedLowerer::new()
        .unwrap()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"s","card":"unit-follower"}]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "head-occurrences",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &typed).unwrap();
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("pending")
    };
    assert_eq!(options.len(), 2);
    let head = options
        .iter()
        .find(|option| option.line == 2)
        .unwrap()
        .handle;
    let declaration = awaiting(
        game.answer_internal_card(pending.actor, pending.token, Answer::Pending(head))
            .unwrap(),
    );
    assert_eq!(
        game.answer_internal_card(
            declaration.actor,
            declaration.token,
            Answer::OptionalCost(false)
        )
        .unwrap(),
        CardPlay::Cancelled
    );
    game.resume_internal_card().unwrap();
    let remaining = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &remaining.domain else {
        panic!("remaining")
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].line, 3);
    let result = resolved(
        game.answer_internal_card(
            remaining.actor,
            remaining.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22
    );
    assert_eq!(
        result
            .progress
            .facts
            .iter()
            .filter(|fact| matches!(fact, Fact::AbilityPlayed { .. }))
            .count(),
        1
    );
}

#[test]
fn variable_public_materials_reject_conflicting_quantities_and_unavailable_paid_results() {
    let valid = json!({
        "cost":{"return_to_hand":{"sel":{"who":"self","zone":"field","other":true},"any":true,"result":"materials"}},
        "do":[{"draw":{"n":{"x_paid":"materials"}}}]
    });
    let lowerer = ExpandedLowerer::new().unwrap();
    lowerer
        .lower(&expanded("unit-follower", "fanfare", valid.clone()))
        .unwrap();
    for material in [
        json!({"sel":{"who":"self","zone":"field"},"any":true,"n":1_u8,"result":"materials"}),
        json!({"sel":{"who":"self","zone":"field"},"n":0_u8,"result":"materials"}),
        json!({"sel":{"who":"self","zone":"field"},"any":false,"result":"materials"}),
    ] {
        let mut body = valid.clone();
        body["cost"]["return_to_hand"] = material;
        lowerer
            .lower(&expanded("unit-follower", "fanfare", body))
            .unwrap_err();
    }
    let mut unknown = valid.clone();
    unknown["do"][0]["draw"]["n"] = json!({"x_paid":"unknown"});
    lowerer
        .lower(&expanded("unit-follower", "fanfare", unknown))
        .unwrap_err();
    let mut early = valid;
    early["targets"] = json!([{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower"},"n":{"x_paid":"materials"}}]);
    lowerer
        .lower(&expanded("unit-follower", "fanfare", early))
        .unwrap_err();
}

fn field_ex_token_cost() -> Value {
    json!({"cost":{"steps":[{"act_self":true},{"to_cemetery":{
        "sel":{"who":"self","zone":["field","ex"],"trait":"魔界","token":true},"n":1
    }}]},"do":[{"life":{"who":"self","delta":1}}]})
}

#[test]
fn field_ex_token_cemetery_material_accepts_both_zone_orders_and_keeps_other_shapes_closed() {
    for zones in [json!(["field", "ex"]), json!(["ex", "field"])] {
        let mut body = field_ex_token_cost();
        body["cost"]["steps"][1]["to_cemetery"]["sel"]["zone"] = zones;
        let typed = program("unit-follower", "activate", body);
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "token-material-admission",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &typed).unwrap();
    }
    for (key, value) in [
        ("who", json!("opp")),
        ("who", json!("all")),
        ("zone", json!(["field", "cemetery"])),
        ("kind", json!("follower")),
        ("kind", json!("amulet")),
        ("trait", json!("")),
        ("token", json!(false)),
        ("name", json!("material")),
        ("other", json!(true)),
        ("orig_cost_le", json!(3)),
    ] {
        let mut body = field_ex_token_cost();
        body["cost"]["steps"][1]["to_cemetery"]["sel"][key] = value;
        lowerer()
            .lower(&expanded("unit-follower", "activate", body))
            .unwrap_err();
    }
    for key in ["token", "trait"] {
        let mut body = field_ex_token_cost();
        body["cost"]["steps"][1]["to_cemetery"]["sel"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        lowerer()
            .lower(&expanded("unit-follower", "activate", body))
            .unwrap_err();
    }
    for (key, value) in [
        ("n", json!(2)),
        ("result", json!("paid")),
        ("cost_sum_ge", json!(1)),
    ] {
        let mut body = field_ex_token_cost();
        body["cost"]["steps"][1]["to_cemetery"][key] = value;
        lowerer()
            .lower(&expanded("unit-follower", "activate", body))
            .unwrap_err();
    }
}

fn field_ex_material_catalog() -> Arc<Catalog> {
    let (snapshot, keywords, effects) = catalog_documents(&json!("1"));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            match row["number"].as_str().unwrap() {
                "unit-amulet" => {
                    row["faces"][0]["card_type"] = json!("アミュレット・トークン");
                    row["faces"][0]["traits"] = json!(["魔界"]);
                }
                "unit-follower" => row["faces"][0]["traits"] = json!(["魔界"]),
                "unit-spell" => {
                    row["faces"][0]["card_type"] = json!("スペル・トークン");
                    row["faces"][0]["traits"] = json!(["反例"]);
                }
                _ => {}
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), effects)]).unwrap(),
    )
}

#[test]
fn field_ex_token_cemetery_material_accepts_amulet_tokens_and_pays_atomically() {
    let catalog = field_ex_material_catalog();
    let typed = program("unit-follower", "activate", field_ex_token_cost());
    for zone in ["field", "ex"] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([
            {"id":"ordinary","card":"unit-follower"},{"id":"wrong-trait","card":"unit-spell"}
        ]);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"private","card":"unit-amulet"}]);
        initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"foreign","card":"unit-amulet"}]);
        initial["players"]["P1"]["zones"][zone]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"material","card":"unit-amulet"}));
        let mut game = Game::new(
            Arc::clone(&catalog),
            &initial,
            &Value::Null,
            &Value::Null,
            "amulet-token-material",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &typed).unwrap();
        game.load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        game.load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let choice = awaiting(
            game.activate_internal_card(
                "s",
                typed.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap(),
        );
        let InputDomain::CostMaterials(materials) = &choice.domain else {
            panic!("public token payment")
        };
        assert_eq!(
            materials.members.len(),
            1,
            "Q1685 admits the own Makai amulet token, excluding ordinary, other-trait, private and opposing cards"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["acted"],
            false,
            "the ordered act cost waits for the full material choice"
        );
        resolved(
            game.answer_internal_card(
                choice.actor,
                choice.token,
                Answer::CostMaterials(vec![materials.members[0].handle]),
            )
            .unwrap(),
        );
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(
            after["objects"]["s"]["acted"], true,
            "the act cost commits with the token material"
        );
        assert!(
            after["objects"].get("material").is_none(),
            "the paid token is erased after reaching the cemetery"
        );
        assert_eq!(
            after["P1"]["leader"]["life"], 21_u8,
            "the body resolves only after the whole cost was paid"
        );
        assert_eq!(
            after["P2"]["ex"],
            json!(["foreign"]),
            "opposing public tokens cannot pay"
        );
        assert_eq!(
            after["P1"]["hand"],
            json!(["private"]),
            "hidden hand cards cannot pay this public cost"
        );
    }
}

#[test]
fn field_ex_token_cemetery_material_loader_rejects_other_destinations_counts_and_traits() {
    let typed = program("unit-follower", "activate", field_ex_token_cost());
    for change in 0..4 {
        let mut altered = typed.card().clone();
        let play = altered.abilities[0].node.play.as_mut().unwrap();
        if change == 0 {
            let CostSpec::Ordered { steps } = &mut play.costs else {
                panic!("ordered payment")
            };
            let Action::Move { movement, .. } = &mut steps[1].instruction.action else {
                panic!("material move")
            };
            movement.to = MoveDestination::Owners {
                zone: OwnerZone::Hand,
            };
        } else {
            let ChoiceDomain::Objects { constraint, .. } = &mut play.parameters[0].domain else {
                panic!("material domain")
            };
            if change == 1 {
                let count = Cardinality {
                    minimum: Nat(2),
                    maximum: Some(Nat(2)),
                };
                constraint.cardinality = count;
                constraint.groups[0].cardinality = count;
            } else {
                let Selector::Union { sets } = &mut constraint.groups[0].selector else {
                    panic!("public union")
                };
                if change == 2 {
                    let Selector::Zone { predicate, .. } = &mut sets[1] else {
                        panic!("public leaf")
                    };
                    **predicate = Expr::Literal(Bool(true));
                } else {
                    let mut body = field_ex_token_cost();
                    body["cost"]["steps"][1]["to_cemetery"]["sel"]["trait"] = json!("反例");
                    let other = program("unit-follower", "activate", body);
                    let ChoiceDomain::Objects {
                        constraint: other_constraint,
                        ..
                    } = &other.card().abilities[0]
                        .node
                        .play
                        .as_ref()
                        .unwrap()
                        .parameters[0]
                        .domain
                    else {
                        panic!("other material domain")
                    };
                    let Selector::Union { sets: other_sets } = &other_constraint.groups[0].selector
                    else {
                        panic!("other public union")
                    };
                    sets[1] = other_sets[1].clone();
                }
            }
        }
        let checked = WellFormedCardProgram::check(altered).unwrap();
        let mut game = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "closed-material-union",
        )
        .unwrap();
        game.load_internal_card("unit-follower", &checked)
            .unwrap_err();
    }
}
