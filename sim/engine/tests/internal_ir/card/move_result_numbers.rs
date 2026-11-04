use super::*;
use alloc::collections::BTreeMap;

fn circle(number: &Value) -> Value {
    equipment_played::tree(
        &json!({"fanfare":{"do":[
            {"move":{"top":{"n":1,"who":"self"},"to":"ex","result":"placed"}},
            {"life":{"who":"self","delta":number}}
        ]}}),
        "unit-follower",
    )
}
fn sea(number: &Value) -> Value {
    equipment_played::tree(
        &json!({"spell":{"do":[
            {"look":{"bind":"seen","top":{"n":5,"who":"self"}}},
            {"choose":{"bind":"picked","from":{"in":"seen","kind":"follower","trait":"海洋"},"upto":1}},
            {"move":{"ref":"picked","to":"field","result":"entered"}},
            {"move":{"ref":{"in":"seen","except":{"in":{"objects_of":"entered","view":"before"}}},"to":"deck","position":"bottom","order":"chooser"}},
            {"damage":{"to":{"who":"opp","zone":"field","kind":"follower"},"n":number}}
        ]}}),
        "unit-spell",
    )
}
fn load(tree: &Value, number: &str) {
    let programs = lowerer()
        .lower_with_printed_costs(tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
        .unwrap();
    let mut board = Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "move-result-numbers-preflight",
    )
    .unwrap();
    board.load_internal_card(number, &programs[number]).unwrap();
}
#[test]
fn ordinary_top_deck_to_ex_with_literal_followup_is_a_valid_lower_load_baseline() {
    load(&circle(&json!(1)), "unit-follower");
}
#[test]
fn ordinary_optional_marine_field_entry_and_ordered_remainder_is_a_valid_lower_load_baseline() {
    load(&sea(&json!(1)), "unit-spell");
}
#[test]
fn existing_public_field_follower_life_sum_is_a_valid_lower_load_baseline() {
    load(
        &sea(
            &json!({"agg":{"of":{"who":"self","zone":"field","kind":"follower"},"attr":"hp","fn":"sum"}}),
        ),
        "unit-spell",
    );
}
fn sum(name: &str, attr: &str) -> Value {
    json!({"agg":{"of":{"in":{"follow":name}},"attr":attr,"fn":"sum"}})
}

fn activated_circle(number: &Value) -> Value {
    let mut tree = circle(number);
    let entry = &mut tree["cards"]["unit-follower"]["abilities"][0];
    let body = entry.as_object_mut().unwrap().remove("fanfare").unwrap();
    entry["activate"] = json!({"cost":{"pp":0},"do":body["do"]});
    tree
}

fn synthetic_catalog(cost: u8) -> Arc<Catalog> {
    let (snapshot, registry, document) = catalog_documents(&json!(cost.to_string()));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-follower" {
                row["faces"][0]["traits"] = json!(["海洋"]);
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    )
}

#[test]
fn ex_result_heals_only_for_the_actual_arrivals_original_cost() {
    let mut tree = activated_circle(&sum("placed", "orig_cost"));
    tree["cards"]["unit-follower"]["abilities"][0]["activate"]["do"].as_array_mut().unwrap().insert(1,json!({"apply":{"on":{"follow":"placed"},"effect":{"cost_mod":{"when_playing":{"in":"affected"},"delta":-2}},"until":"turn"}}));
    let program = lowerer().lower(&tree).unwrap();
    for (empty, full, cost) in [
        (false, false, 4_u8),
        (true, false, 4),
        (false, true, 4),
        (false, false, 0),
    ] {
        let mut initial = setup(
            1,
            if empty {
                json!([])
            } else {
                json!([{"id":"placed","card":"unit-spell","state":{"cost":0}}])
            },
        );
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        if full {
            initial["players"]["P1"]["zones"]["ex"] = json!(
                (0_u8..5)
                    .map(|n| json!({"id":format!("old{n}"),"card":"unit-spell"}))
                    .collect::<Vec<_>>()
            );
        }
        let mut board = Game::new(
            synthetic_catalog(cost),
            &initial,
            &Value::Null,
            &Value::Null,
            "ex-result",
        )
        .unwrap();
        board
            .load_internal_card("unit-follower", &program["unit-follower"])
            .unwrap();
        board
            .load_internal_card("unit-spell", &card(vec![]))
            .unwrap();
        let result = resolved(
            board
                .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
                .unwrap(),
        );
        let healed = if empty || full { 0_u8 } else { cost };
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], 20_u8 + healed);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::LifeChange(_)))
                .count(),
            usize::from(healed > 0)
        );
        if !empty {
            assert_eq!(view["objects"]["placed"]["generation"], u8::from(!full));
            assert_eq!(
                view["P1"]["deck"],
                if full { json!(["placed"]) } else { json!([]) }
            );
        }
    }
}

#[expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the closed Look and remainder scenario."
)]
fn finish_sea(board: &mut Game, mut play: CardPlay, eligible: bool, accept: bool) -> Activation {
    use sve_engine::game::internal::InputDomain;
    while let CardPlay::Awaiting(q) = play {
        let answer = if let InputDomain::Targets(domain) = &q.domain {
            assert_eq!(
                domain.members.len(),
                usize::from(eligible),
                "only the marine follower is eligible"
            );
            assert!(
                board.internal_decision(View::P2).is_none(),
                "the opposing player cannot inspect the private Look choice"
            );
            Answer::Targets(if accept {
                vec![domain.members[0].handle]
            } else {
                vec![]
            })
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|c| c.handle).collect())
        } else {
            panic!("private Look or remainder order");
        };
        play = board
            .answer_internal_card(q.actor, q.token, answer)
            .unwrap();
    }
    resolved(play)
}

#[test]
fn field_result_damages_for_actual_entry_and_decline_or_full_field_reads_zero() {
    for (accept, full, delta, eligible) in [
        (true, false, 0_i64, true),
        (false, false, 0, true),
        (true, true, 0, true),
        (true, false, 2, true),
        (true, false, -1, true),
        (true, false, -4, true),
        (false, false, 0, false),
    ] {
        let mut tree = sea(&sum("entered", "hp"));
        if delta != 0 {
            tree["cards"]["unit-spell"]["abilities"][0]["spell"]["do"]
                .as_array_mut()
                .unwrap()
                .insert(3, json!({"buff":{"on":{"follow":"entered"},"hp":delta}}));
        }
        let programs = lowerer()
            .lower_with_printed_costs(&tree, &BTreeMap::from([("unit-spell".to_owned(), Nat(1))]))
            .unwrap();
        let mut initial = setup(
            1,
            json!([{"id":"entrant","card":"unit-follower"},{"id":"rest","card":"unit-amulet"}]),
        );
        if !eligible {
            initial["players"]["P1"]["zones"]["deck"][0]["card"] = json!("unit-spell");
        }
        initial["players"]["P1"]["zones"]["field"] = if full {
            json!(
                (0_u8..5)
                    .map(|n| json!({"id":format!("old{n}"),"card":"unit-follower"}))
                    .collect::<Vec<_>>()
            )
        } else {
            json!([])
        };
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"hp":10});
        let mut board = Game::new(
            synthetic_catalog(1),
            &initial,
            &Value::Null,
            &Value::Null,
            "field-result",
        )
        .unwrap();
        board
            .load_internal_card("unit-spell", &programs["unit-spell"])
            .unwrap();
        board
            .load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        board
            .load_internal_card("unit-amulet", &card(vec![]))
            .unwrap();
        let play = board.play_internal_card("s", PlayerId { seat: 1 }).unwrap();
        let result = finish_sea(&mut board, play, eligible, accept);
        let damage = if accept && !full {
            u8::try_from((3_i64 + delta).max(0)).unwrap()
        } else {
            0_u8
        };
        let view = board.projection(View::Referee).unwrap();
        assert_eq!(view["objects"]["f"]["hp"], 10_u8 - damage);
        assert_eq!(
            result
                .progress
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::Damage(_)))
                .count(),
            usize::from(damage > 0)
        );
        assert_eq!(
            view["P1"]["deck"],
            if accept && !full {
                json!(["rest"])
            } else {
                json!(["rest", "entrant"])
            }
        );
    }
}

#[test]
fn followed_sums_reject_wrong_properties_functions_filters_and_unknown_results() {
    for attr in ["cost", "power", "hp"] {
        assert_eq!(
            lowerer()
                .lower(&circle(&sum("placed", attr)))
                .unwrap_err()
                .code,
            "unsupported_construct"
        );
    }
    for attr in ["cost", "power", "orig_cost"] {
        assert_eq!(
            lowerer()
                .lower(&sea(&sum("entered", attr)))
                .unwrap_err()
                .code,
            "unsupported_construct"
        );
    }
    for function in ["min", "max"] {
        let mut number = sum("placed", "orig_cost");
        number["agg"]["fn"] = json!(function);
        lowerer().lower(&circle(&number)).unwrap_err();
    }
    for of in [
        json!({"in":{"follow":"missing"}}),
        json!({"in":{"objects_of":"placed","view":"before"}}),
        json!({"in":{"follow":"placed"},"kind":"follower"}),
        json!({"in":{"follow":"placed","where":true}}),
    ] {
        lowerer()
            .lower(&circle(
                &json!({"agg":{"of":of,"attr":"orig_cost","fn":"sum"}}),
            ))
            .unwrap_err();
    }
    let mut escaped = activated_circle(&sum("placed", "orig_cost"));
    let steps = &mut escaped["cards"]["unit-follower"]["abilities"][0]["activate"]["do"];
    steps[0] = json!({"optional":{"do":[steps[0].clone()]}});
    lowerer().lower(&escaped).unwrap_err();
    let mut cross = activated_circle(&sum("placed", "orig_cost"));
    let mut second = cross["cards"]["unit-follower"]["abilities"][0].clone();
    second["line"] = json!(2);
    second["activate"]["do"] = json!([{"life":{"who":"self","delta":1}}]);
    cross["cards"]["unit-follower"]["abilities"]
        .as_array_mut()
        .unwrap()
        .push(second);
    lowerer().lower(&cross).unwrap();
    cross["cards"]["unit-follower"]["abilities"][1]["activate"]["do"][0]["life"]["delta"] =
        sum("placed", "orig_cost");
    lowerer().lower(&cross).unwrap_err();
}

#[test]
fn raw_follow_sum_cannot_bypass_native_producer_property_or_wf_guards() {
    let original = lowerer()
        .lower(&activated_circle(&sum("placed", "orig_cost")))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for property in [
        NumericProperty::Cost,
        NumericProperty::Attack,
        NumericProperty::Life,
        NumericProperty::Count,
    ] {
        let mut raw = original.card().clone();
        for step in &mut raw.abilities[0].node.body.as_mut().unwrap().steps {
            if let Node::Instruction { instruction } = &mut step.node
                && let Action::Life {
                    change:
                        NumericChange::Add {
                            amount: Expr::Compute(operation),
                        },
                    ..
                } = &mut instruction.action
                && let IntOperation::Sum {
                    property: actual, ..
                } = operation.as_mut()
            {
                *actual = property;
            }
        }
        let checked = WellFormedCardProgram::check(raw).unwrap();
        let mut board = Game::new(
            catalog(),
            &setup(1, json!([])),
            &Value::Null,
            &Value::Null,
            "raw-follow-rejection",
        )
        .unwrap();
        board
            .load_internal_card("unit-follower", &checked)
            .unwrap_err();
    }
}

#[test]
fn search_and_token_outputs_do_not_authorize_followed_sums() {
    for producer in [
        json!({"search":{"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1,"to":"ex","result":"placed"}}),
        json!({"create":{"token":"unit-token","to":"ex","n":1,"result":"placed"}}),
    ] {
        let mut input = activated_circle(&sum("placed", "orig_cost"));
        input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][0] = producer;
        let mut prefix = input.clone();
        prefix["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        lowerer().lower(&prefix).unwrap();
        assert_eq!(
            lowerer().lower(&input).unwrap_err().code,
            "unsupported_construct"
        );
    }
}

#[test]
fn followed_population_cannot_be_used_as_a_public_extremum() {
    let mut input = activated_circle(&sum("placed", "orig_cost"));
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"][1] = json!({"destroy":{"ref":{"who":"opp","zone":"field","kind":"follower","where":{"cmp":{"left":{"cost_of":"item"},"op":"eq","right":{"agg":{"of":{"in":{"follow":"placed"}},"attr":"cost","fn":"min"}}}}}}});
    assert_eq!(
        lowerer().lower(&input).unwrap_err().code,
        "unsupported_construct"
    );
}

#[test]
fn raw_follow_permission_kind_unknown_receipts_and_non_deck_producers_stay_closed() {
    let original = lowerer()
        .lower(&activated_circle(&sum("placed", "orig_cost")))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for case in 0_u8..6 {
        let mut raw = original.card().clone();
        for step in &mut raw.abilities[0].node.body.as_mut().unwrap().steps {
            if let Node::Instruction { instruction } = &mut step.node {
                if case == 5
                    && let Action::Move { movement, .. } = &mut instruction.action
                {
                    movement.to = MoveDestination::Owners {
                        zone: OwnerZone::Cemetery,
                    };
                }
                if let Action::Life {
                    change:
                        NumericChange::Add {
                            amount: Expr::Compute(operation),
                        },
                    ..
                } = &mut instruction.action
                    && let IntOperation::Sum {
                        objects:
                            Selector::MoveAfter {
                                receipt,
                                permission,
                            },
                        ..
                    } = operation.as_mut()
                {
                    match case {
                        0 => {
                            *permission = FollowPermission::TriggerExplicitReference {
                                event: EventContext::Trigger {},
                            }
                        }
                        1 => {
                            *permission = FollowPermission::RuleCarry {
                                rule: RuleCarry::ExToResolutionEffect,
                            }
                        }
                        2 => {
                            *receipt = MovementReceipt::Search {
                                receipt: ReceiptReference::new(ReceiptId(1)),
                            }
                        }
                        3 => {
                            *receipt = MovementReceipt::Move {
                                receipt: ReceiptReference::new(ReceiptId(999_999)),
                            }
                        }
                        4 => {
                            *receipt = MovementReceipt::Banish {
                                receipt: ReceiptReference::new(ReceiptId(1)),
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if let Ok(checked) = WellFormedCardProgram::check(raw) {
            let mut board = Game::new(
                catalog(),
                &setup(1, json!([])),
                &Value::Null,
                &Value::Null,
                "raw-follow-guard",
            )
            .unwrap();
            board
                .load_internal_card("unit-follower", &checked)
                .unwrap_err();
        }
    }
}
