use super::*;

fn change(operation: CounterOperation, amount: u64) -> Action {
    Action::Counter {
        holder: Holder::Object {
            object: ObjectOperand::SourceHolder {},
        },
        kind: CounterKind("test".into()),
        operation,
        amount: Expr::Literal(Nat(amount)),
    }
}

fn read() -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Live(Box::new(NatRead::Counter {
            holder: Holder::Object {
                object: ObjectOperand::SourceHolder {},
            },
            kind: CounterKind("test".into()),
        })),
    }))
}

#[test]
fn effect_removal_stops_at_zero_and_later_reads_observe_the_changed_counter() {
    let a = ability(
        1,
        vec![
            instruction(1, change(CounterOperation::Remove {}, 5)),
            instruction(2, change(CounterOperation::Add {}, 1)),
            draw_value(3, read()),
        ],
    );
    let mut game = position_with_state(1, &card(vec![a]), &json!({"counters":{"test":2}}));
    let result = activate(&mut game, 1);
    cards(&result.progress, 3, 1, 1);
    assert!(
        matches!(&result.progress.receipts[&ReceiptId(1)], InstructionOutcome::Counter(event)
        if event.before == Nat(2) && event.after == Nat(0) && event.delta == Int(-2))
    );

    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["counters"]["test"],
        1
    );
}

#[test]
fn repeated_counter_costs_require_the_total_and_roll_back_earlier_payments() {
    let a = payment::paid_ability(
        vec![
            payment::pp(1),
            payment::act(),
            change(CounterOperation::Remove {}, 1),
            change(CounterOperation::Remove {}, 1),
        ],
        vec![draw(1, 1)],
    );
    let program = card(vec![a]);
    for count in [1, 2] {
        let mut game = position_with_state(1, &program, &json!({"counters":{"test":count}}));
        let before = exact_authority(&game);
        let result = game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 });
        if count == 1 {
            assert!(matches!(
                result.unwrap_err(),
                sve_engine::EngineFailure::IllegalPlay(_)
            ));
            assert_eq!(exact_authority(&game), before);
        } else {
            let result = resolved(result.unwrap());
            cards(&result.progress, 1, 1, 1);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["s"]["counters"]["test"],
                0
            );
        }
    }
}

#[test]
fn overflow_and_stale_source_counter_reads_reject_atomically() {
    for (steps, count) in [
        (
            vec![instruction(1, change(CounterOperation::Add {}, 1))],
            i64::MAX,
        ),
        (
            vec![
                instruction(1, payment::self_to_cemetery()),
                draw_value(2, read()),
            ],
            2,
        ),
    ] {
        let mut game = position_with_state(
            1,
            &card(vec![ability(1, steps)]),
            &json!({"counters":{"test":count}}),
        );
        let before = exact_authority(&game);
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}
