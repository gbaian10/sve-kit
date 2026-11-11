use super::*;
use alloc::collections::BTreeMap;
use sve_engine::game::internal::InputDomain;

fn read() -> Value {
    json!({"power_of":{"objects_of":"fee","view":"before"}})
}

#[test]
fn paid_material_result_cannot_be_redeclared_by_a_later_body_producer() {
    for (mut producers, operation, field) in [
        (
            vec![json!({"move":{"top":{"who":"self","n":1},"to":"cemetery","result":"fee"}})],
            "move",
            "result",
        ),
        (
            vec![
                json!({"search":{"sel":{"who":"self","zone":"deck","kind":"follower"},"n":1,"to":"hand","result":"fee"}}),
            ],
            "search",
            "result",
        ),
        (
            vec![json!({"create":{"token":"token-a","to":"field","n":1,"result":"fee"}})],
            "create",
            "result",
        ),
        (
            vec![json!({"look":{"top":{"who":"self","n":1},"bind":"fee"}})],
            "look",
            "bind",
        ),
        (
            vec![json!({"destroy":{"ref":"this","result":"fee"}})],
            "destroy",
            "result",
        ),
        (
            vec![
                json!({"look":{"top":{"who":"self","n":1},"bind":"seen"}}),
                json!({"choose":{"bind":"fee","from":{"in":"seen"},"n":1}}),
            ],
            "choose",
            "bind",
        ),
    ] {
        let with_producers = |producers: &[Value]| {
            let mut document = tree(&fee(), &read());
            document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
                .as_array_mut()
                .unwrap()
                .splice(0..0, producers.iter().cloned());
            document
        };
        assert!(
            lowerer().lower(&with_producers(&producers)).is_err(),
            "a later body producer cannot change the meaning of an existing paid-material name"
        );
        producers.last_mut().unwrap()[operation][field] = json!("another");
        assert!(
            lowerer().lower(&with_producers(&producers)).is_ok(),
            "the otherwise supported producer remains valid with a fresh name"
        );
    }
}

fn fee() -> Value {
    json!({"to_cemetery":{"sel":{"who":"self","zone":"field","kind":"follower","other":true},"n":1,"result":"fee"}})
}

fn tree(cost: &Value, amount: &Value) -> Value {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] =
        json!({"cost":cost,"do":[{"damage":{"to":{"leaders":"opponents"},"n":amount}}]});
    tree["cards"] = json!({"unit-follower":seed});
    tree
}

#[test]
fn material_attack_reads_the_modified_field_value_after_moving_for_both_seats() {
    let program = lowerer().lower(&tree(&fee(), &read())).unwrap()["unit-follower"].clone();
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = setup(seat, json!([]));
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["zones"]["hand"] = json!([]);
        initial["players"][actor]["zones"]["field"] = json!([
            {"id":"s","card":"unit-follower"},
            {"id":"material","card":"unit-follower","state":{"power":9}}
        ]);
        let mut board = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "material-power",
        )
        .unwrap();
        board.load_internal_card("unit-follower", &program).unwrap();
        let CardPlay::Awaiting(question) = board
            .activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap()
        else {
            panic!("materials")
        };
        let InputDomain::CostMaterials(domain) = &question.domain else {
            panic!("materials")
        };
        assert_eq!(
            domain.members.len(),
            1,
            "the source cannot pay the other-follower cost"
        );
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(vec![domain.members[0].handle]),
                )
                .unwrap(),
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(
            state[actor]["cemetery"],
            json!(["material"]),
            "payment really moves the material"
        );
        assert_eq!(
            state[opponent]["leader"]["life"], 11,
            "damage keeps the actual field attack rather than rereading the cemetery printing"
        );
    }
}

#[test]
fn material_attack_rejects_unknown_multi_other_zone_and_nonpayment_results() {
    for cost in [
        json!({"to_cemetery":{"sel":{"who":"self","zone":"field","kind":"follower"},"n":2,"result":"fee"}}),
        json!({"banish":{"sel":{"who":"self","zone":"field","kind":"follower"},"n":1,"result":"fee"}}),
        json!({"to_cemetery":{"sel":{"who":"self","zone":"ex","kind":"follower"},"n":1,"result":"fee"}}),
        json!({"act_self":true}),
    ] {
        assert!(
            lowerer().lower(&tree(&cost, &read())).is_err(),
            "only the supported paid single field follower supplies an attack"
        );
    }
    let after = json!({"power_of":{"objects_of":"fee","view":"after"}});
    assert!(
        lowerer().lower(&tree(&fee(), &after)).is_err(),
        "only before is opened"
    );
    let mut body_move = tree(&json!({"pp":0}), &read());
    body_move["cards"]["unit-follower"]["abilities"][0]["activate"]["do"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            json!({"move":{"ref":"this","to":"cemetery","result":"fee"}}),
        );
    assert!(
        lowerer().lower(&body_move).is_err(),
        "an effect move cannot masquerade as a cost receipt"
    );
}

#[test]
fn typed_material_attack_requires_a_single_cost_material_parameter() {
    let original = lowerer().lower(&tree(&fee(), &read())).unwrap()["unit-follower"]
        .card()
        .clone();
    for change_count in [true, false] {
        let mut program = original.clone();
        let decision = &mut program.abilities[0].node.play.as_mut().unwrap().parameters[0];
        if change_count {
            let ChoiceDomain::Objects { constraint, .. } = &mut decision.domain else {
                panic!("materials")
            };
            constraint.cardinality = Cardinality {
                minimum: Nat(2),
                maximum: Some(Nat(2)),
            };
            constraint.groups[0].cardinality = constraint.cardinality;
        } else {
            let DecisionKind::PlayParameter { slot, .. } = &mut decision.kind else {
                panic!("parameter")
            };
            *slot = PlaySlot::Targets;
        }
        assert!(
            WellFormedCardProgram::check(program).is_err(),
            "unrelated or multiple-member selections do not prove a paid scalar"
        );
    }
}

#[test]
fn exact_material_name_lists_do_not_open_public_exists_or_exchange_queries() {
    let names = json!(["first", "second"]);
    let mut cost = fee();
    cost["to_cemetery"]["sel"]["names"] = names.clone();
    assert!(
        lowerer().lower(&tree(&cost, &read())).is_ok(),
        "one own field follower may match either exact name"
    );
    let mut exists = tree(&json!({"pp":0}), &json!(0));
    exists["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([{"if":{"cond":{"exists":{"who":"self","zone":"field","kind":"follower","names":names}},"then":[{"draw":{"n":1}}]}}]);
    assert!(
        lowerer().lower(&exists).is_err(),
        "public existence does not inherit the material-only name list"
    );
    let mut exchange = tree(&json!({"pp":0}), &json!(0));
    let mut exchange_cost = cost.clone();
    exchange_cost["to_cemetery"]["sel"]
        .as_object_mut()
        .unwrap()
        .remove("other");
    exchange_cost["to_cemetery"]
        .as_object_mut()
        .unwrap()
        .remove("result");
    let ability = exchange["cards"]["unit-follower"]["abilities"][0]
        .as_object_mut()
        .unwrap();
    ability.remove("activate");
    ability.insert(
        "play".into(),
        json!({"optional_cost":exchange_cost,"optional_cost_delta":-1}),
    );
    assert!(
        lowerer()
            .lower_with_printed_costs(
                &exchange,
                &BTreeMap::from([("unit-follower".into(), Nat(3))])
            )
            .is_err(),
        "exchange does not inherit the material-only name list"
    );
    exchange["cards"]["unit-follower"]["abilities"][0]["play"]["optional_cost"]["to_cemetery"]["sel"].as_object_mut().unwrap().remove("names");
    assert!(
        lowerer()
            .lower_with_printed_costs(
                &exchange,
                &BTreeMap::from([("unit-follower".into(), Nat(3))])
            )
            .is_ok(),
        "the same fixed exchange is otherwise supported"
    );
    cost["to_cemetery"]["sel"]["zone"] = json!("ex");
    assert!(
        lowerer().lower(&tree(&cost, &read())).is_err(),
        "the exact-name material opening stays on the field"
    );
}

#[test]
fn negative_material_attack_is_preserved_for_comparison_and_deals_no_damage() {
    let mut document = tree(&fee(), &read());
    document["cards"]["unit-follower"]["abilities"][0]["activate"]["do"].as_array_mut().unwrap().push(json!({"if":{"cond":{"cmp":{"left":read(),"op":"eq","right":-3}},"then":[{"life":{"who":"self","delta":1}}]}}));
    let program = lowerer().lower(&document).unwrap()["unit-follower"].clone();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"},{"id":"material","card":"unit-follower","state":{"power":-3}}]);
    let mut board = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "negative-paid-power",
    )
    .unwrap();
    board.load_internal_card("unit-follower", &program).unwrap();
    let CardPlay::Awaiting(question) = board
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("materials")
    };
    let result = resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["material"]),
        "the valid negative-attack follower still pays the cost"
    );
    assert_eq!(
        state["P1"]["leader"]["life"], 21,
        "numeric comparisons retain the negative before value"
    );
    assert_eq!(
        state["P2"]["leader"]["life"], 20,
        "nonpositive damage changes no life"
    );
    assert!(
        !result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Damage(_))),
        "zero damage creates no damage event"
    );
}

#[test]
fn paid_material_cost_and_attack_share_the_before_projection_in_arithmetic() {
    let cost = json!({"cost_of":{"objects_of":"fee","view":"before"}});
    let amount = json!({"plus":[{"times":[cost,2]},read()]});
    let program = lowerer().lower(&tree(&fee(), &amount)).unwrap()["unit-follower"].clone();
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut initial = setup(seat, json!([]));
        initial["turn"]["active"] = json!(actor);
        initial["players"][actor]["zones"]["hand"] = json!([]);
        initial["players"][actor]["zones"]["field"] = json!([
            {"id":"s","card":"unit-follower"},
            {"id":"material","card":"unit-follower","state":{"cost":7,"power":3}}
        ]);
        let mut board = Game::new(
            catalog(),
            &initial,
            &Value::Null,
            &Value::Null,
            "paid-material-number",
        )
        .unwrap();
        board.load_internal_card("unit-follower", &program).unwrap();
        let CardPlay::Awaiting(question) = board
            .activate_internal_card("s", AbilityId(1), PlayerId { seat })
            .unwrap()
        else {
            panic!("materials")
        };
        resolved(
            board
                .answer_internal_card(
                    question.actor,
                    question.token,
                    Answer::CostMaterials(vec![0]),
                )
                .unwrap(),
        );
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state[actor]["cemetery"], json!(["material"]));
        assert_eq!(
            state[opponent]["leader"]["life"], 3,
            "two times the frozen effective cost plus the frozen attack, after both modifiers have disappeared"
        );
    }
}

#[test]
fn zero_paid_material_cost_remains_available_to_a_comparison() {
    let mut input = tree(&fee(), &json!(0));
    input["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"if":{"cond":{"cmp":{"left":{"cost_of":{"objects_of":"fee","view":"before"}},"op":"eq","right":0}},"then":[{"life":{"who":"self","delta":1}}]}}
    ]);
    let program = lowerer().lower(&input).unwrap()["unit-follower"].clone();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower"},
        {"id":"material","card":"unit-follower","state":{"cost":0}}
    ]);
    let mut board = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "zero-paid-material-cost",
    )
    .unwrap();
    board.load_internal_card("unit-follower", &program).unwrap();
    let CardPlay::Awaiting(question) = board
        .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("materials")
    };
    resolved(
        board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::CostMaterials(vec![0]),
            )
            .unwrap(),
    );
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["leader"]["life"], 21,
        "zero is a saved numeric value, not missing information or the printed cost"
    );
}

#[test]
fn paid_material_cost_keeps_cardinality_zone_timing_and_paid_branch_restrictions() {
    let read = json!({"cost_of":{"objects_of":"fee","view":"before"}});
    for cost in [
        json!({"to_cemetery":{"sel":{"who":"self","zone":"field","kind":"follower"},"n":2,"result":"fee"}}),
        json!({"to_cemetery":{"sel":{"who":"self","zone":"field","kind":"follower"},"any":true,"result":"fee"}}),
        json!({"banish":{"sel":{"who":"self","zone":"field","kind":"follower"},"n":1,"result":"fee"}}),
        json!({"to_cemetery":{"sel":{"who":"self","zone":"ex","kind":"follower"},"n":1,"result":"fee"}}),
        json!({"pp":0}),
    ] {
        lowerer().lower(&tree(&cost, &read)).unwrap_err();
    }
    for amount in [
        json!({"cost_of":{"objects_of":"fee","view":"after"}}),
        json!({"cost_of":{"objects_of":"unknown","view":"before"}}),
    ] {
        lowerer().lower(&tree(&fee(), &amount)).unwrap_err();
    }
    let mut optional = tree(&fee(), &read);
    let ability = optional["cards"]["unit-follower"]["abilities"][0]
        .as_object_mut()
        .unwrap();
    let mut body = ability.remove("activate").unwrap();
    body.as_object_mut().unwrap().remove("cost");
    let mut cost = fee();
    cost["result"] = json!("paid");
    body["optional_cost"] = cost;
    ability.insert("fanfare".into(), body);
    assert!(
        lowerer().lower(&optional).is_err(),
        "an optional unpaid branch cannot produce a frozen cost scalar"
    );
    let mut early = tree(&fee(), &read);
    early["cards"]["unit-follower"]["abilities"][0]["activate"]["targets"] = json!([{"bind":"enemy","from":{"who":"opp","zone":"field","kind":"follower","cost_le":read},"n":1}]);
    assert!(
        lowerer().lower(&early).is_err(),
        "formal targets precede payment"
    );
}

#[test]
fn raw_paid_material_number_does_not_open_other_properties_or_future_payments() {
    let read = json!({"cost_of":{"objects_of":"fee","view":"before"}});
    let original = lowerer().lower(&tree(&fee(), &read)).unwrap()["unit-follower"]
        .card()
        .clone();
    for property in [
        NumericProperty::Count,
        NumericProperty::Life,
        NumericProperty::OriginalCost,
    ] {
        let mut raw = original.clone();
        let Node::Instruction { instruction } =
            &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
        else {
            panic!("damage")
        };
        let Action::Damage {
            amount:
                DamageAmount::Uniform {
                    amount: Expr::Frozen(value),
                },
            ..
        } = &mut instruction.action
        else {
            panic!("scalar")
        };
        let IntFrozen::PaidMaterialNumber {
            property: target, ..
        } = value.as_mut()
        else {
            panic!("material")
        };
        *target = property;
        assert!(
            WellFormedCardProgram::check(raw).is_err(),
            "the receipt only retains the supported cost and attack projections"
        );
    }
    let mut no_payment = original;
    no_payment.abilities[0].node.play.as_mut().unwrap().costs = CostSpec::Ordered { steps: vec![] };
    assert!(
        WellFormedCardProgram::check(no_payment).is_err(),
        "an object selection alone cannot supply a paid snapshot"
    );
}
