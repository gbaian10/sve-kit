#![expect(
    clippy::panic,
    reason = "Unexpected fixture structure invalidates these counter scenarios."
)]

use super::*;
use core::mem;
use sve_engine::EngineFailure;
use sve_engine::game::internal::InputDomain;

fn change(operation: CounterOperation, amount: Expr<Nat>) -> Action {
    Action::Counter {
        holder: Holder::Object {
            object: ObjectOperand::SourceHolder {},
        },
        kind: CounterKind("test".into()),
        operation,
        amount,
    }
}

fn checked(value: Expr<Int>) -> Expr<Nat> {
    Expr::Compute(Box::new(NatOperation::CheckedInt { value }))
}

fn integer(value: Expr<Nat>) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::FromNat { value }))
}

fn arithmetic(operation: Arithmetic, arguments: Vec<Expr<Int>>) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation,
        arguments,
    }))
}

fn count(zone_kind: ZoneKind) -> Expr<Nat> {
    let mut selector = zone(zone_kind, 1);
    if let Selector::Zone { binding, .. } = &mut selector {
        *binding = Binding::new(if zone_kind == ZoneKind::Ex { 10 } else { 9 }, ScopeId(1));
    }
    let Selector::Zone { players, .. } = &mut selector else {
        panic!("fixture structure")
    };
    players.relation = PlayerRelation::SelfPlayer;
    Expr::Live(Box::new(NatRead::ZoneCount { selector }))
}

fn source_counter() -> Expr<Nat> {
    Expr::Live(Box::new(NatRead::Counter {
        holder: Holder::Object {
            object: ObjectOperand::SourceHolder {},
        },
        kind: CounterKind("test".into()),
    }))
}

#[test]
fn composed_counter_amounts_read_hand_after_draw_both_seats() {
    let amount = checked(arithmetic(
        Arithmetic::Add,
        vec![integer(count(ZoneKind::Hand)), Expr::Literal(Int(1))],
    ));
    for seat in [1, 2] {
        let program = card(vec![ability(
            1,
            vec![
                draw(1, 1),
                instruction(2, change(CounterOperation::Add {}, amount.clone())),
            ],
        )]);
        let mut game = position_with_state(seat, &program, &json!({"counters":{"test":2}}));
        let result = activate(&mut game, seat);
        assert!(
            matches!(&result.progress.receipts[&ReceiptId(2)], InstructionOutcome::Counter(event)
            if event.before == Nat(2) && event.after == Nat(4) && event.delta == Int(2))
        );
    }
}

#[test]
fn zero_counter_amount_has_a_receipt_and_remove_still_saturates() {
    for (operation, amount, expected, delta) in [
        (CounterOperation::Add {}, count(ZoneKind::Hand), 2, 0),
        (
            CounterOperation::Remove {},
            checked(arithmetic(
                Arithmetic::Add,
                vec![integer(count(ZoneKind::Hand)), Expr::Literal(Int(3))],
            )),
            0,
            -2,
        ),
    ] {
        let program = card(vec![ability(
            1,
            vec![instruction(1, change(operation, amount))],
        )]);
        let mut game = position_with_state(1, &program, &json!({"counters":{"test":2}}));
        let result = activate(&mut game, 1);
        assert!(
            matches!(&result.progress.receipts[&ReceiptId(1)], InstructionOutcome::Counter(event)
            if event.before == Nat(2) && event.after == Nat(expected) && event.delta == Int(delta))
        );
    }
}

#[test]
fn each_counter_instruction_reads_the_latest_counter_once() {
    let program = card(vec![ability(
        1,
        vec![
            instruction(
                1,
                change(
                    CounterOperation::Add {},
                    checked(arithmetic(
                        Arithmetic::Add,
                        vec![integer(source_counter()), Expr::Literal(Int(1))],
                    )),
                ),
            ),
            instruction(2, change(CounterOperation::Remove {}, source_counter())),
        ],
    )]);
    let mut game = position_with_state(1, &program, &json!({"counters":{"test":2}}));
    let result = activate(&mut game, 1);
    assert!(
        matches!(&result.progress.receipts[&ReceiptId(1)], InstructionOutcome::Counter(event) if event.after == Nat(5))
    );
    assert!(
        matches!(&result.progress.receipts[&ReceiptId(2)], InstructionOutcome::Counter(event) if event.before == Nat(5) && event.after == Nat(0) && event.delta == Int(-5))
    );
}

#[test]
fn a_counter_formula_overflow_restores_draws_counters_and_all_authority() {
    let amount = checked(arithmetic(
        Arithmetic::Add,
        vec![Expr::Literal(Int(i64::MAX)), integer(count(ZoneKind::Hand))],
    ));
    let program = card(vec![ability(
        1,
        vec![
            draw(1, 1),
            instruction(2, change(CounterOperation::Add {}, Expr::Literal(Nat(1)))),
            instruction(3, change(CounterOperation::Add {}, amount)),
        ],
    )]);
    let mut game = position_with_state(1, &program, &json!({"counters":{"test":2}}));
    let before = exact_authority(&game);
    assert!(matches!(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 }),
        Err(EngineFailure::Invalid(_))
    ));
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn unknown_signed_and_out_of_range_literal_counter_amounts_stay_closed_at_load() {
    for amount in [
        checked(Expr::Literal(Int(-1))),
        checked(arithmetic(
            Arithmetic::Subtract,
            vec![integer(count(ZoneKind::Hand)), Expr::Literal(Int(1))],
        )),
        Expr::Literal(Nat(u64::MAX)),
    ] {
        let program = card(vec![ability(
            1,
            vec![instruction(1, change(CounterOperation::Add {}, amount))],
        )]);
        let mut game = unloaded();
        let before = exact_authority(&game);
        assert!(matches!(
            game.load_internal_card("unit-follower", &program),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn dynamic_counter_effects_do_not_authorize_dynamic_counter_payments() {
    for amount in [count(ZoneKind::Hand), checked(Expr::Literal(Int(0)))] {
        let program = card(vec![payment::paid_ability(
            vec![payment::pp(1), change(CounterOperation::Remove {}, amount)],
            vec![draw(1, 1)],
        )]);
        let mut game = unloaded();
        let before = exact_authority(&game);
        let result = game.load_internal_card("unit-follower", &program);
        assert!(
            matches!(result, Err(EngineFailure::Unsupported(_))),
            "{result:?}"
        );
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn paid_pp_x_is_reused_by_the_counter_without_a_second_answer() {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "x":{},"cost":{"steps":[{"pp":1},{"pp":"x"},{"pp":"x"}]},
            "do":[{"draw":{"who":"self","n":"x"}}]
        }),
    )]);
    let mut raw = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    let Node::Instruction { instruction } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("fixture structure")
    };
    let Action::Draw { count, .. } = &instruction.action else {
        panic!("fixture structure")
    };
    instruction.action = change(CounterOperation::Add {}, checked(count.clone()));
    let program = WellFormedCardProgram::check(raw).unwrap();
    for x in [0, 3] {
        let mut initial = setup(1, json!([]));
        initial["players"]["P1"]["pp"] = json!({"current":7,"max":7});
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"s","card":"unit-follower","state":{"counters":{"test":0}}}]);
        let mut game = evolution_linked_movement::loaded(&initial, &program);
        let CardPlay::Awaiting(question) = game
            .activate_internal_card(
                "s",
                program.card().abilities[0].node.id,
                PlayerId { seat: 1 },
            )
            .unwrap()
        else {
            panic!("one PP X choice")
        };
        assert_eq!(
            question.domain,
            InputDomain::Integer(Cardinality {
                minimum: Nat(0),
                maximum: Some(Nat(3))
            })
        );
        let before = exact_authority(&game);
        game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(4)))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        resolved(
            game.answer_internal_card(question.actor, question.token, Answer::Integer(Nat(x)))
                .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 6 - 2 * x);
        assert_eq!(view["objects"]["s"]["counters"]["test"], x);
    }
}

#[test]
fn bp20_020_ir_counter_reuses_the_paid_fusion_members_after_the_source_moves_to_ex() {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "fusion":{"sel":{"who":"self","zone":["hand","ex"],"kind":"spell"},"upto":2,"result":"materials"},
            "do":[{"draw":{"who":"self","n":{"count":{"in":{"objects_of":"materials","view":"before"},"kind":"spell"}}}}]
        }),
    )]);
    let mut raw = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap()
        .card()
        .clone();
    let Node::Instruction { instruction } =
        &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("fixture structure")
    };
    let Action::Draw { count, .. } = &instruction.action else {
        panic!("fixture structure")
    };
    instruction.action = change(CounterOperation::Add {}, checked(count.clone()));
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"unit-follower"},{"id":"h","card":"unit-spell"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"e","card":"unit-spell"}]);
    let mut game = evolution_linked_movement::loaded(&initial, &program);
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let CardPlay::Awaiting(question) = game
        .activate_internal_card(
            "source",
            program.card().abilities[0].node.id,
            PlayerId { seat: 1 },
        )
        .unwrap()
    else {
        panic!("one Fusion payment")
    };
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("Fusion members")
    };
    assert_eq!(domain.members.len(), 2);
    let handles = domain.members.iter().map(|member| member.handle).collect();
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(handles),
        )
        .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["source"]["counters"]["test"], 2);
    assert_eq!(view["P1"]["ex"], json!(["source"]));
}

fn batch(amount: Expr<Nat>) -> WellFormedCardProgram {
    let binding = Binding::new(1, ScopeId(2));
    let mut action = change(CounterOperation::Add {}, amount);
    let Action::Counter { holder, .. } = &mut action else {
        panic!("fixture structure")
    };
    *holder = Holder::Object {
        object: ObjectOperand::Bound { binding },
    };
    card(vec![ability(
        1,
        vec![Located {
            source: source("unit-follower"),
            node: Node::Control {
                control: Control::EachObject {
                    input: zone(ZoneKind::Field, 90),
                    binding,
                    order: None,
                    body: body(2, vec![instruction(1, action)]),
                },
            },
        }],
    )])
}

fn batch_game(program: &WellFormedCardProgram, later: i64) -> Game {
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower","state":{"counters":{"test":1}}},
        {"id":"later","card":"unit-follower","state":{"counters":{"test":later}}}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    evolution_linked_movement::loaded(&initial, program)
}

#[test]
fn batch_counter_amounts_read_the_start_state_before_any_holder_changes() {
    let mut game = batch_game(&batch(source_counter()), 3);
    activate(&mut game, 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["s"]["counters"]["test"], 2);
    assert_eq!(view["objects"]["later"]["counters"]["test"], 4);
}

#[test]
fn batch_counter_amounts_can_read_their_bound_holders() {
    let amount = Expr::Live(Box::new(NatRead::Counter {
        holder: Holder::Object {
            object: ObjectOperand::Bound {
                binding: Binding::new(1, ScopeId(2)),
            },
        },
        kind: CounterKind("test".into()),
    }));
    let mut game = batch_game(&batch(amount), 3);
    activate(&mut game, 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["s"]["counters"]["test"], 2);
    assert_eq!(view["objects"]["later"]["counters"]["test"], 6);
}

#[test]
fn dynamic_batch_overflow_restores_earlier_holders_and_execution_authority() {
    let mut game = batch_game(&batch(source_counter()), i64::MAX);
    let before = exact_authority(&game);
    assert!(matches!(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 }),
        Err(EngineFailure::Invalid(_))
    ));
    assert_eq!(exact_authority(&game), before);
}

fn unloaded() -> Game {
    Game::new(
        catalog(),
        &setup(1, json!([])),
        &Value::Null,
        &Value::Null,
        "counter-admission",
    )
    .unwrap()
}

#[test]
fn counter_amounts_do_not_open_a_new_whole_card_integer_decision() {
    let binding = Binding::new(1, ScopeId(1));
    let program = card(vec![ability(
        1,
        vec![
            integer_choice(20, 1),
            instruction(1, change(CounterOperation::Add {}, Expr::Bound(binding))),
        ],
    )]);
    let mut game = unloaded();
    let before = exact_authority(&game);
    assert!(
        matches!(game.load_internal_card("unit-follower", &program), Err(EngineFailure::Unsupported(message)) if message.contains("whole-card synchronous node"))
    );
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn filtered_hidden_counter_counts_fail_without_changing_authority() {
    let mut amount = count(ZoneKind::Hand);
    let Expr::Live(read) = &mut amount else {
        panic!("fixture structure")
    };
    let NatRead::ZoneCount {
        selector: Selector::Zone { predicate, .. },
    } = read.as_mut()
    else {
        panic!("fixture structure")
    };
    **predicate = Expr::Literal(Bool(false));
    let program = card(vec![ability(
        1,
        vec![instruction(1, change(CounterOperation::Add {}, amount))],
    )]);
    let mut game = position_with_card(1, &program);
    let before = exact_authority(&game);
    assert!(matches!(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 }),
        Err(EngineFailure::Unsupported(_))
    ));
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn selected_and_quick_counter_fees_cannot_borrow_dynamic_effect_amounts() {
    let tree = evolution_linked_movement::tree(&[(
        "activate",
        json!({
            "cost":{"counter":{"on":{"who":"self","zone":"field","name":"unit-amulet"},"holders":1,"name":"test","n":2}},
            "do":[{"draw":{"who":"self","n":1}}]
        }),
    )]);
    let selected = lowerer()
        .lower(&tree)
        .unwrap()
        .remove("unit-follower")
        .unwrap();
    for quick in [false, true] {
        let mut raw = selected.card().clone();
        let ability = &mut raw.abilities[0];
        if quick {
            ability.node.source.origin = AbilityOrigin::Keyword {
                keyword: KeywordId::Quick,
                origin: Box::new(ability.node.source.origin.clone()),
            };
            locate_steps(
                &mut ability.node.body.as_mut().unwrap().steps,
                &ability.source,
                &ability.node.source,
            );
        }
        let CostSpec::Ordered { steps } = &mut ability.node.play.as_mut().unwrap().costs else {
            panic!("fixture structure")
        };
        for step in steps {
            step.instruction.source = ability.node.source.clone();
            if let Action::Counter { amount, .. } = &mut step.instruction.action {
                *amount = checked(Expr::Literal(Int(2)));
            }
        }
        let program = WellFormedCardProgram::check(raw).unwrap();
        let mut game = unloaded();
        let before = exact_authority(&game);
        let error = game
            .load_internal_card("unit-follower", &program)
            .unwrap_err();
        assert!(matches!(error, EngineFailure::Unsupported(_)), "{error:?}");
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn optional_counter_fees_keep_their_fixed_amount_guard() {
    let mut a = payment::paid_ability(
        vec![change(
            CounterOperation::Remove {},
            checked(Expr::Literal(Int(1))),
        )],
        vec![],
    );
    let costs = mem::replace(
        &mut a.node.play.as_mut().unwrap().costs,
        CostSpec::Ordered { steps: vec![] },
    );
    a.node.body = Some(body(
        1,
        vec![Located {
            source: a.source.clone(),
            node: Node::Control {
                control: Control::Pay {
                    at: PaymentTime::ResolutionChoice,
                    costs,
                    optional: PaymentOptionality::Optional,
                    on_paid: body(2, vec![draw(1, 1)]),
                    receipt: ReceiptId(200),
                },
            },
        }],
    ));
    locate_steps(
        &mut a.node.body.as_mut().unwrap().steps,
        &a.source,
        &a.node.source,
    );
    let program = card(vec![a]);
    let mut game = unloaded();
    let before = exact_authority(&game);
    let error = game
        .load_internal_card("unit-follower", &program)
        .unwrap_err();
    assert!(matches!(error, EngineFailure::Unsupported(_)), "{error:?}");
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn counter_natural_conditionals_keep_public_composition_and_lazy_evaluation() {
    let condition = Expr::Compute(Box::new(Predicate::CompareNat {
        left: count(ZoneKind::Ex),
        comparison: Comparison::Equal,
        right: Expr::Literal(Nat(0)),
    }));
    let overflow = checked(arithmetic(
        Arithmetic::Add,
        vec![Expr::Literal(Int(i64::MAX)), Expr::Literal(Int(1))],
    ));
    let amount = Expr::Compute(Box::new(NatOperation::Conditional {
        condition,
        then_value: checked(arithmetic(
            Arithmetic::Minimum,
            vec![integer(count(ZoneKind::Hand)), Expr::Literal(Int(2))],
        )),
        else_value: overflow,
    }));
    let program = card(vec![ability(
        1,
        vec![instruction(1, change(CounterOperation::Add {}, amount))],
    )]);
    let mut game = position_with_state(1, &program, &json!({"counters":{"test":1}}));
    let result = activate(&mut game, 1);
    assert!(
        matches!(&result.progress.receipts[&ReceiptId(1)], InstructionOutcome::Counter(event) if event.delta == Int(0))
    );
}

#[test]
fn a_bound_counter_read_does_not_expand_the_existing_move_consumer() {
    let amount = Expr::Live(Box::new(NatRead::Counter {
        holder: Holder::Object {
            object: ObjectOperand::Bound {
                binding: Binding::new(1, ScopeId(2)),
            },
        },
        kind: CounterKind("test".into()),
    }));
    let mut raw = batch(amount).card().clone();
    let Node::Control {
        control: Control::EachObject { body, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("counter projection")
    };
    let Node::Instruction { instruction } = &mut body.steps[0].node else {
        panic!("counter instruction")
    };
    let Action::Counter { operation, .. } = &mut instruction.action else {
        panic!("counter action")
    };
    *operation = CounterOperation::Move {
        to: Holder::Object {
            object: ObjectOperand::SourceHolder {},
        },
    };
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut game = unloaded();
    let before = exact_authority(&game);
    assert!(
        matches!(game.load_internal_card("unit-follower", &program), Err(EngineFailure::Unsupported(message)) if message.contains("private observation"))
    );
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn dynamic_batch_removal_saturates_even_for_a_quantity_above_the_signed_range() {
    let amount = Expr::Compute(Box::new(NatOperation::Conditional {
        condition: Expr::Literal(Bool(true)),
        then_value: Expr::Literal(Nat(u64::MAX)),
        else_value: Expr::Literal(Nat(0)),
    }));
    let mut raw = batch(amount).card().clone();
    let Node::Control {
        control: Control::EachObject { body, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("counter projection")
    };
    let Node::Instruction { instruction } = &mut body.steps[0].node else {
        panic!("counter instruction")
    };
    let Action::Counter { operation, .. } = &mut instruction.action else {
        panic!("counter action")
    };
    *operation = CounterOperation::Remove {};
    let program = WellFormedCardProgram::check(raw).unwrap();
    let mut game = batch_game(&program, 3);
    activate(&mut game, 1);
    let view = game.projection(View::Referee).unwrap();
    for holder in ["s", "later"] {
        assert_eq!(view["objects"][holder]["counters"]["test"], 0);
    }
}
