use super::*;

fn arithmetic(operation: Arithmetic, left: i64, right: i64) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation,
        arguments: vec![Expr::Literal(Int(left)), Expr::Literal(Int(right))],
    }))
}

fn set_amount(id: u64, value: Expr<Int>) -> Located<Node> {
    modify(
        id,
        zone(ZoneKind::Field, 900_u64.saturating_add(id)),
        CharacteristicPatch::Numeric {
            stat: Stat::Attack,
            change: NumericChange::Set { amount: value },
        },
        DurationEnd::NoTemporalExpiry {},
    )
}

#[test]
fn constant_set_compositions_keep_value_and_same_timestamp_conflicts() {
    for value in [
        Expr::Literal(Int(4)),
        arithmetic(Arithmetic::Add, 2, 2),
        Expr::Compute(Box::new(IntOperation::Conditional {
            condition: Expr::Compute(Box::new(Predicate::CompareInt {
                left: arithmetic(Arithmetic::Add, 1, 1),
                comparison: Comparison::Equal,
                right: Expr::Literal(Int(2)),
            })),
            then_value: Expr::Literal(Int(4)),
            else_value: Expr::Literal(Int(5)),
        })),
    ] {
        let mut board = game(1, 0);
        let second = set_amount(2, Expr::Literal(Int(4)));
        let progress = run(&mut board, vec![set_amount(1, value.clone()), second], 1);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["f"]["power"],
            4
        );
        assert_eq!(changes(&progress).len(), 1);
        let incompatible = set_amount(2, Expr::Literal(Int(5)));
        unchanged(
            &mut board,
            &checked(vec![set_amount(1, value), incompatible]),
            1,
        );
    }
}

#[test]
fn unproved_negative_and_overflowing_set_compositions_preserve_authority() {
    for value in [
        arithmetic(Arithmetic::Subtract, 0, 1),
        arithmetic(Arithmetic::Add, i64::MAX, 1),
        source_attack(CharacteristicView::Effective),
        Expr::Compute(Box::new(IntOperation::Conditional {
            condition: Expr::Literal(Bool(true)),
            then_value: Expr::Literal(Int(1)),
            else_value: Expr::Literal(Int(-1)),
        })),
    ] {
        let mut board = follower_source(1);
        unchanged(&mut board, &checked(vec![set_amount(1, value)]), 1);
    }
}

#[test]
fn add_constant_errors_stay_at_execution_and_unselected_branches_stay_lazy() {
    for selected in [false, true] {
        let mut board = game(1, 3);
        let origin = board.clone();
        let amount = Expr::Compute(Box::new(IntOperation::Conditional {
            condition: Expr::Literal(Bool(selected)),
            then_value: arithmetic(Arithmetic::Add, i64::MAX, 1),
            else_value: Expr::Literal(Int(1)),
        }));
        let pending = run(
            &mut board,
            vec![
                draw(1, 1),
                integer_choice(2, 1),
                modify(
                    3,
                    zone(ZoneKind::Field, 903),
                    patch(Stat::Attack, amount),
                    DurationEnd::NoTemporalExpiry {},
                ),
            ],
            1,
        );
        assert!(
            !pending.complete,
            "admission must not evaluate the later Add"
        );
        let question = board.internal_decision(View::P1).unwrap();
        let published = board.clone();
        let result = board.answer_internal(
            PlayerId { seat: 1 },
            question.token,
            Answer::Integer(Nat(0)),
        );
        if selected {
            assert!(matches!(result, Err(sve_engine::EngineFailure::Invalid(_))));
            transactions::assert_reverted_from(&board, &origin, &published);
            let restored = exact_authority(&board);
            board
                .answer_internal(
                    PlayerId { seat: 1 },
                    question.token,
                    Answer::Integer(Nat(0)),
                )
                .unwrap_err();
            assert_eq!(exact_authority(&board), restored);
        } else {
            assert!(result.unwrap().complete);
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["f"]["power"],
                3
            );
        }
    }
}
