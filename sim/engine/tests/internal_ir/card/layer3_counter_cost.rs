use super::*;
use sve_engine::game::internal::InputDomain;

fn input(shared: bool) -> Value {
    let mut body = json!({"cost":{"steps":[{"pp":1},{"counter":{"on":"this","name":"test","any":true,"result":"removed"}}]},
        "do":[{"buff":{"on":"this","power":{"x_paid":"removed"},"hp":{"x_paid":"removed"}}}]});
    if shared {
        body["x"] = json!({});
        body["cost"]["steps"] = json!([
            {"pp":"x"},{"act_self":true},{"counter":{"on":"this","name":"test","n":"x","result":"removed"}}
        ]);
    }
    evolution_linked_movement::tree(&[("activate", body)])
}

#[test]
fn counter_cost_declares_one_affordable_count_and_the_body_reads_the_actual_paid_count() {
    for (shared, count, pp, maximum) in [(true, 2, 3, 2), (false, 3, 2, 3)] {
        let program = lowerer()
            .lower(&input(shared))
            .unwrap()
            .remove("unit-follower")
            .unwrap();
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"s","card":"unit-follower","state":{"counters":{"test":count}}}]);
        let mut game = evolution_linked_movement::loaded(&initial, &program);
        let CardPlay::Awaiting(q) = game
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("counter declaration");
        };
        assert_eq!(
            q.domain,
            InputDomain::Integer(Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(maximum))
            })
        );
        let before = exact_authority(&game);
        game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(maximum + 1)))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        resolved(
            game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(2)))
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 1);
        assert_eq!(state["objects"]["s"]["counters"]["test"], count - 2);
        assert_eq!(state["objects"]["s"]["power"], 4);
        assert_eq!(state["objects"]["s"]["hp"], 5);
        assert_eq!(state["objects"]["s"]["acted"], shared);
    }
}

#[test]
fn zero_counter_payment_is_legal_and_fixed_cost_failure_precedes_declaration() {
    let program = lowerer()
        .lower(&input(false))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for (count, pp) in [(0, 1), (3, 0)] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":pp});
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"s","card":"unit-follower","state":{"counters":{"test":count}}}]);
        let mut game = evolution_linked_movement::loaded(&initial, &program);
        let before = exact_authority(&game);
        let play = game.activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        );
        if pp == 0 {
            assert!(matches!(
                play.unwrap_err(),
                sve_engine::EngineFailure::IllegalPlay(_)
            ));
            assert_eq!(exact_authority(&game), before);
        } else {
            let CardPlay::Awaiting(q) = play.unwrap() else {
                panic!("zero declaration");
            };
            assert_eq!(
                q.domain,
                InputDomain::Integer(Cardinality {
                    minimum: Nat(0),
                    maximum: Some(Nat(0))
                })
            );
            resolved(
                game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(0)))
                    .unwrap(),
            );
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], 0);
            assert_eq!(state["objects"]["s"]["power"], 2);
            assert_eq!(state["objects"]["s"]["hp"], 3);
        }
    }
}

#[test]
fn repeated_counter_costs_and_fixed_parts_share_one_budget_and_refusals_restore_authority() {
    let mut value = input(true);
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"]["steps"] = json!([
        {"pp":"x"},{"act_self":true},
        {"counter":{"on":"this","name":"test","n":1}},
        {"counter":{"on":"this","name":"test","n":"x","result":"removed"}},
        {"counter":{"on":"this","name":"test","n":"x"}}
    ]);
    let program = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"counters":{"test":6}}}]);
    let mut game = evolution_linked_movement::loaded(&initial, &program);
    let CardPlay::Awaiting(q) = game
        .activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("repeated budget");
    };
    assert_eq!(
        q.domain,
        InputDomain::Integer(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(2))
        })
    );
    let before = exact_authority(&game);
    game.answer_internal_card(PlayerId { seat: 2 }, q.token, Answer::Integer(Nat(1)))
        .unwrap_err();
    game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(u64::MAX)))
        .unwrap_err();
    assert_eq!(exact_authority(&game), before);
    resolved(
        game.answer_internal_card(q.actor, q.token, Answer::Integer(Nat(2)))
            .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 2);
    assert_eq!(state["objects"]["s"]["counters"]["test"], 1);
    assert_eq!(state["objects"]["s"]["power"], 4);
}

#[test]
fn an_acted_source_cannot_declare_counter_x_and_multiple_any_costs_keep_separate_receipts() {
    let shared = lowerer()
        .lower(&input(true))
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","state":{"acted":true,"counters":{"test":4}}}]);
    let mut game = evolution_linked_movement::loaded(&initial, &shared);
    let before = exact_authority(&game);
    assert!(matches!(
        game.activate_internal_card(
            "s",
            shared.card().abilities[0].node.id,
            PlayerId { seat: 1 }
        )
        .unwrap_err(),
        sve_engine::EngineFailure::IllegalPlay(_)
    ));
    assert_eq!(exact_authority(&game), before);

    let mut value = input(false);
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"]["steps"] = json!([
        {"pp":1}, {"counter":{"on":"this","name":"test","any":true,"result":"first"}},
        {"counter":{"on":"this","name":"test","any":true,"result":"second"}}
    ]);
    value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
        {"buff":{"on":"this","power":{"x_paid":"first"},"hp":{"x_paid":"second"}}}
    ]);
    let program = lowerer()
        .lower(&value)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    initial["players"]["P1"]["zones"]["field"][0]["state"]["acted"] = json!(false);
    let mut other = evolution_linked_movement::loaded(&initial, &program);
    let CardPlay::Awaiting(first) = other
        .activate_internal_card(
            "s",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("first counter declaration")
    };
    let CardPlay::Awaiting(second) = other
        .answer_internal_card(first.actor, first.token, Answer::Integer(Nat(3)))
        .unwrap()
    else {
        panic!("second counter declaration")
    };
    assert_eq!(
        second.domain,
        InputDomain::Integer(Cardinality {
            minimum: Nat(0),
            maximum: Some(Nat(1))
        })
    );
    resolved(
        other
            .answer_internal_card(second.actor, second.token, Answer::Integer(Nat(1)))
            .unwrap(),
    );
    let after = other.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["s"]["power"], 5);
    assert_eq!(after["objects"]["s"]["hp"], 4);
    assert_eq!(after["objects"]["s"]["counters"]["test"], 0);
}

#[test]
fn counter_payment_names_and_receipt_producers_remain_closed() {
    for (shared, patch) in [
        (
            false,
            json!({"on":"this","name":"test","upto":2,"result":"removed"}),
        ),
        (
            false,
            json!({"on":{"who":"self","zone":"field"},"holders":1,"name":"test","any":true}),
        ),
        (
            true,
            json!({"on":"this","name":"test","n":"x","result":"x"}),
        ),
    ] {
        let mut value = input(shared);
        let step = if shared { 2 } else { 1 };
        value["cards"]["unit-follower"]["abilities"][0]["activate"]["cost"]["steps"][step] =
            json!({"counter":patch});
        if shared {
            value["cards"]["unit-follower"]["abilities"][0]["activate"]["do"] = json!([
                {"buff":{"on":"this","power":{"x_paid":"x"}}}
            ]);
        }
        lowerer().lower(&value).unwrap_err();
    }
    let amount = Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Frozen(Box::new(NatFrozen::ReceiptActual {
            receipt: MeasuredReceipt::Counters {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
        })),
    }));
    let program = body(
        1,
        vec![
            instruction(
                1,
                Action::Counter {
                    holder: Holder::Object {
                        object: ObjectOperand::SourceHolder {},
                    },
                    kind: CounterKind("test".into()),
                    operation: CounterOperation::Remove {},
                    amount: Expr::Literal(Nat(1)),
                },
            ),
            draw_value(2, amount),
        ],
    );
    WellFormedProgram::check(program).unwrap_err();
}
