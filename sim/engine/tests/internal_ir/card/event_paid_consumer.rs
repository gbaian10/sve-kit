#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the paid event scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain};

fn lower(steps: &Value) -> WellFormedCardProgram {
    lowerer()
        .lower(&equipment_played::tree(
            &json!({"auto":{
                "on":{"enter_field":{"who":"opp","sel":{"kind":"follower"}}},
                "do":steps
            }}),
            "unit-amulet",
        ))
        .unwrap()
        .remove("unit-amulet")
        .unwrap()
}

fn paid_subject() -> WellFormedCardProgram {
    let program = lower(&json!([{"pay":{"cost":{"act_self":true},"do":[
        {"move":{"ref":"it","to":"cemetery"}},
        {"draw":{"n":1}},
        {"life":{"who":"self","delta":1}},
        {"if":{"cond":{"entered_from":{"ref":"it","from":"hand"}},"then":[{"life":{"who":"self","delta":1}}]}}
    ]}}]));
    let mut raw = program.card().clone();
    let Node::Control {
        control: Control::Pay { on_paid, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("resolution payment")
    };
    let Node::Instruction { instruction: draw } = &on_paid.steps[1].node else {
        panic!("paid draw")
    };
    let receipt = draw.receipt;
    let Node::Instruction { instruction: life } = &mut on_paid.steps[2].node else {
        panic!("receipt consumer")
    };
    let Action::Life {
        change: NumericChange::Add { amount },
        ..
    } = &mut life.action
    else {
        panic!("life delta")
    };
    *amount = Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Frozen(Box::new(NatFrozen::ReceiptActual {
            receipt: MeasuredReceipt::Cards {
                receipt: ReceiptReference::new(receipt),
            },
        })),
    }));
    WellFormedCardProgram::check(raw).unwrap()
}

fn state(acted: bool) -> Value {
    let mut state = setup(2, json!([{"id":"opponent-draw","card":"unit-spell"}]));
    state["turn"]["active"] = json!("P2");
    state["players"]["P1"]["zones"]["hand"] = json!([]);
    state["players"]["P1"]["zones"]["deck"] = json!([{"id":"paid-draw","card":"unit-spell"}]);
    state["players"]["P1"]["zones"]["field"] =
        json!([{"id":"observer","card":"unit-amulet","state":{"acted":acted}}]);
    state["players"]["P2"]["zones"]["field"] = json!([]);
    state["players"]["P2"]["zones"]["hand"] = json!([{"id":"incoming","card":"unit-follower"}]);
    state
}

fn board(program: &WellFormedCardProgram, acted: bool) -> Game {
    let mut game = Game::new(
        static_keywords::keyword_catalog(),
        &state(acted),
        &Value::Null,
        &Value::Null,
        "paid-event-subject",
    )
    .unwrap();
    game.load_internal_card("unit-amulet", program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

fn question(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(question) = play else {
        panic!("resolution payment question")
    };
    assert_eq!(
        question.domain,
        InputDomain::OptionalCost,
        "each resolution payment asks independently"
    );
    assert_eq!(
        question.actor,
        PlayerId { seat: 1 },
        "the source controller pays despite the opponent causing the event"
    );
    question
}

fn payment(game: &mut Game) -> DecisionView {
    assert!(
        matches!(
            game.play_internal_card("incoming", PlayerId { seat: 2 })
                .unwrap(),
            CardPlay::Entered(_)
        ),
        "the opponent's follower enters before the source controller pays"
    );
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("entry occurrence")
    };
    question(
        game.answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    )
}

#[test]
fn only_successful_payment_reads_the_subject_origin_and_local_draw_receipt() {
    let program = paid_subject();
    for (accept, acted, paid) in [
        (true, false, true),
        (false, false, false),
        (true, true, false),
    ] {
        let mut game = board(&program, acted);
        let cost = payment(&mut game);
        let result = resolved(
            game.answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(accept))
                .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["leader"]["life"], if paid { 22 } else { 20 });
        assert_eq!(
            view["P1"]["hand"],
            if paid {
                json!(["paid-draw"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            view["P2"]["field"],
            if paid { json!([]) } else { json!(["incoming"]) }
        );
        assert_eq!(
            view["P2"]["cemetery"],
            if paid { json!(["incoming"]) } else { json!([]) }
        );
        assert_eq!(view["objects"]["observer"]["acted"], acted || paid);
        assert!(result.progress.receipts.values().any(|receipt| matches!(receipt, InstructionOutcome::ExchangePayment(payment) if payment.complete == Bool(paid) && (paid || payment.steps.is_empty()))), "the actual payment outcome controls the continuation");
        assert_eq!(
            result
                .progress
                .receipts
                .values()
                .any(|receipt| matches!(receipt, InstructionOutcome::Cards(_))),
            paid
        );
    }
}

#[test]
fn a_departed_source_cannot_pay_for_an_event_subject_continuation() {
    let mut raw = paid_subject().card().clone();
    let departure = lower(&json!([{"move":{"ref":"this","to":"cemetery"}}]));
    let mut departure = departure.card().abilities[0]
        .node
        .body
        .as_ref()
        .unwrap()
        .steps[0]
        .clone();
    let Node::Instruction { instruction } = &mut departure.node else {
        panic!("source departure")
    };
    instruction.id = InstructionId(900_001);
    instruction.receipt = ReceiptId(900_002);
    raw.abilities[0]
        .node
        .body
        .as_mut()
        .unwrap()
        .steps
        .insert(0, departure);
    let mut game = board(&WellFormedCardProgram::check(raw).unwrap(), false);
    let cost = payment(&mut game);
    let result = resolved(
        game.answer_internal_card(cost.actor, cost.token, Answer::OptionalCost(true))
            .unwrap(),
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["cemetery"], json!(["observer"]));
    assert_eq!(view["P1"]["leader"]["life"], 20);
    assert_eq!(view["P2"]["field"], json!(["incoming"]));
    assert_eq!(view["P1"]["hand"], json!([]));
    assert!(result.progress.receipts.values().any(|receipt| matches!(receipt, InstructionOutcome::ExchangePayment(payment) if payment.complete == Bool(false) && payment.steps.is_empty())), "a missing source cannot produce a paid continuation");
}

#[test]
fn nested_payments_keep_each_continuation_behind_its_own_payment() {
    let program = lower(&json!([{"pay":{"cost":{"act_self":true},"do":[
        {"pay":{"cost":{"self_to_cemetery":true},"do":[{"move":{"ref":"it","to":"cemetery"}}]}}
    ]}}]));
    for accept_inner in [false, true] {
        let mut game = board(&program, false);
        let outer = payment(&mut game);
        let inner = question(
            game.answer_internal_card(outer.actor, outer.token, Answer::OptionalCost(true))
                .unwrap(),
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["observer"]["acted"],
            true
        );
        resolved(
            game.answer_internal_card(inner.actor, inner.token, Answer::OptionalCost(accept_inner))
                .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(
            view["P2"]["field"],
            if accept_inner {
                json!([])
            } else {
                json!(["incoming"])
            }
        );
        assert_eq!(
            view["P1"]["cemetery"],
            if accept_inner {
                json!(["observer"])
            } else {
                json!([])
            }
        );
    }
}

#[test]
fn paid_local_receipts_do_not_escape_the_optional_continuation() {
    let mut raw = paid_subject().card().clone();
    let body = raw.abilities[0].node.body.as_mut().unwrap();
    let Node::Control {
        control: Control::Pay { on_paid, .. },
    } = &body.steps[0].node
    else {
        panic!("resolution payment")
    };
    let mut leaked = on_paid.steps[2].clone();
    let Node::Instruction { instruction } = &mut leaked.node else {
        panic!("local receipt consumer")
    };
    instruction.id = InstructionId(900_001);
    instruction.receipt = ReceiptId(900_002);
    body.steps.push(leaked);
    assert_eq!(
        WellFormedCardProgram::check(raw).unwrap_err().reason,
        WellFormedReason::UnavailableReceipt
    );
}

#[test]
fn the_event_subject_cannot_replace_the_source_in_a_payment_cost() {
    let mut raw = paid_subject().card().clone();
    let Node::Control {
        control:
            Control::Pay {
                costs: CostSpec::Ordered { steps },
                on_paid,
                ..
            },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("ordered source cost")
    };
    let Node::Instruction {
        instruction: movement,
    } = &on_paid.steps[0].node
    else {
        panic!("event movement")
    };
    let Action::Move { objects, .. } = &movement.action else {
        panic!("event subject")
    };
    let Action::Posture {
        objects: source, ..
    } = &mut steps[0].instruction.action
    else {
        panic!("source act cost")
    };
    source.clone_from(objects);
    let invalid = WellFormedCardProgram::check(raw).unwrap();
    let mut game = board(&paid_subject(), false);
    let error = game
        .load_internal_card("unit-amulet", &invalid)
        .unwrap_err();
    assert!(
        matches!(&error, sve_engine::EngineFailure::Unsupported(message)
            if message.contains("resolution source payment needs a field auto ability")),
        "the subject cannot replace the source in a resolution cost: {error:?}"
    );
}

#[test]
fn a_foreign_event_is_rejected_inside_the_paid_continuation() {
    let mut raw = paid_subject().card().clone();
    let Node::Control {
        control: Control::Pay { on_paid, .. },
    } = &mut raw.abilities[0].node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("payment")
    };
    let Node::Instruction { instruction } = &mut on_paid.steps[0].node else {
        panic!("movement")
    };
    let Action::Move {
        objects: Selector::Objects { objects },
        ..
    } = &mut instruction.action
    else {
        panic!("event subject")
    };
    objects[0] = ObjectOperand::FromExpression {
        value: Box::new(Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject {
            event: Binding::new(999, ScopeId(999)),
        }))),
    };
    assert_eq!(
        WellFormedCardProgram::check(raw).unwrap_err().reason,
        WellFormedReason::UnsupportedShape
    );
}

#[test]
fn paid_grants_cannot_transfer_the_outer_subject_to_an_independent_ability() {
    for reference in ["this", "it"] {
        let tree = equipment_played::tree(
            &json!({"auto":{
                "on":{"enter_field":{"who":"opp","sel":{"kind":"follower"}}},
                "do":[{"pay":{"cost":{"act_self":true},"do":[
                    {"grant":{"on":"this","until":"turn","abilities":[{"auto":{
                        "on":{"phase":{"whose":"self","which":"end"}},
                        "do":[{"move":{"ref":reference,"to":"cemetery"}}]
                    }}]}}
                ]}}]
            }}),
            "unit-amulet",
        );
        let result = lowerer().lower(&tree);
        if reference == "this" {
            result.unwrap();
        } else {
            assert_eq!(result.unwrap_err().code, "unsupported_construct");
        }
    }
}
