use super::*;

fn input() -> Value {
    evolution_linked_movement::tree(&[
        (
            "static",
            json!({"effects":[{"rule_override":{"rule":"counter_cap","on":"this","name":"test","max":3}}]}),
        ),
        (
            "activate",
            json!({"cost":{"pp":0},"do":[
                {"counter":{"on":"this","name":"test","delta":10}},
                {"counter":{"on":"this","name":"other","delta":10}}
            ]}),
        ),
    ])
}

#[test]
fn a_continuous_counter_cap_limits_additions_only_for_its_own_counter_kind() {
    let programs = lowerer().lower(&input()).unwrap();
    let program = &programs["unit-follower"];
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    let mut game = evolution_linked_movement::loaded(&initial, program);
    resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["s"]["counters"]["test"], 3);
    assert_eq!(state["objects"]["s"]["counters"]["other"], 10);
}

#[test]
fn caps_do_not_remove_existing_counters_and_stop_applying_when_the_ability_is_lost() {
    let mut value = input();
    value["cards"]["unit-follower"]["abilities"][1]["activate"]["do"] = json!([
        {"counter":{"on":"this","name":"test","delta":10}},
        {"counter":{"on":"this","name":"test","delta":-2}},
        {"apply":{"on":"this","effect":{"lose_abilities":{}},"until":"turn"}},
        {"counter":{"on":"this","name":"test","delta":10}}
    ]);
    let program = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"counters":{"test":5}}}]);
    let mut game = evolution_linked_movement::loaded(&initial, &program);
    let result = resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    assert!(result.progress.receipts.values().any(|receipt|matches!(receipt,InstructionOutcome::Counter(change) if change.before==Nat(5)&&change.after==Nat(5))));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["counters"]["test"],
        13
    );
}

#[test]
fn a_zero_cap_clamps_before_signed_addition_and_malformed_rules_do_not_load() {
    let mut value = input();
    value["cards"]["unit-follower"]["abilities"][0]["static"]["effects"][0]["rule_override"]["max"] =
        json!(0);
    value["cards"]["unit-follower"]["abilities"][1]["activate"]["do"][0]["counter"]["delta"] =
        json!(0x001F_FFFF_FFFF_FFFF_i64);
    let program = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"counters":{"test":i64::MAX}}}]);
    let mut game = evolution_linked_movement::loaded(&initial, &program);
    resolved(
        game.activate_internal_card(
            "s",
            program.card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["counters"]["test"],
        i64::MAX
    );
    for (key, patch) in [
        ("max", json!(u64::MAX)),
        ("on", json!("other")),
        ("name", json!("")),
    ] {
        let mut malformed = value.clone();
        malformed["cards"]["unit-follower"]["abilities"][0]["static"]["effects"][0]["rule_override"]
            [key] = patch;
        lowerer().lower(&malformed).unwrap_err();
    }
}

#[test]
fn moving_counters_respects_the_receivers_effective_caps_and_preserves_both_endpoints() {
    let mut value = input();
    value["cards"]["unit-follower"]["abilities"][1]["activate"]["targets"] = json!([
        {"bind":"receiver","from":{"who":"self","zone":"field","kind":"follower","other":true},"n":1}
    ]);
    let mut raw = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    let active = &mut raw.abilities[1].node;
    let ChoiceDomain::Objects { binding, .. } = active.play.as_ref().unwrap().parameters[0].domain
    else {
        panic!("receiver binding")
    };
    let steps = &mut active.body.as_mut().unwrap().steps;
    let Node::Instruction { instruction } = &mut steps[0].node else {
        panic!("source counter instruction")
    };
    instruction.action = Action::Counter {
        holder: Holder::Object {
            object: ObjectOperand::SourceHolder {},
        },
        kind: CounterKind("test".into()),
        operation: CounterOperation::Move {
            to: Holder::Object {
                object: ObjectOperand::Selected { selection: binding },
            },
        },
        amount: Expr::Literal(Nat(5)),
    };
    steps.truncate(1);
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower","state":{"counters":{"test":3}}},
        {"id":"receiver","card":"unit-follower","state":{"counters":{"test":2}}}
    ]);
    let mut game = evolution_linked_movement::loaded(&initial, &program);
    let CardPlay::Awaiting(q) = game
        .activate_internal_card(
            "s",
            program.card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("receiver question")
    };
    let result = resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert!(result.progress.receipts.values().any(|receipt|matches!(receipt,InstructionOutcome::CounterMove{from,to} if from.delta==Int(-1)&&to.delta==Int(1))));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["s"]["counters"]["test"], 2);
    assert_eq!(state["objects"]["receiver"]["counters"]["test"], 3);
}

#[test]
fn the_receivers_cap_applies_to_another_sources_addition() {
    let mut value = input();
    let active = &mut value["cards"]["unit-follower"]["abilities"][1]["activate"];
    active["targets"] = json!([{ "bind":"receiver", "from":{"who":"self","zone":"field","kind":"follower","other":true}, "n":1 }]);
    active["do"] = json!([{ "counter":{"on":"receiver","name":"test","delta":10} }]);
    let program = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower","state":{"counters":{"test":0}}},
        {"id":"receiver","card":"unit-follower","state":{"counters":{"test":2}}}
    ]);
    let mut game = evolution_linked_movement::loaded(&initial, &program);
    let CardPlay::Awaiting(q) = game
        .activate_internal_card(
            "s",
            program.card().abilities[1].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("counter recipient")
    };
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["s"]["counters"]["test"], 0);
    assert_eq!(state["objects"]["receiver"]["counters"]["test"], 3);
}
