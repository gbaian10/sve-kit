use super::*;

fn resource(id: u64, amount: i64) -> CostStep {
    let Node::Instruction { mut instruction } = instruction(
        Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(amount)),
        },
        id,
        "synthetic",
    )
    .node
    else {
        panic!("instruction")
    };
    instruction.context = InstructionContext::Cost;
    CostStep {
        keyword: None,
        instruction,
        materials: None,
    }
}

#[test]
fn zero_payment_and_every_alternative_are_checked() {
    accept(body(1, vec![pay(vec![], body(2, vec![]))]));
    accept(body(1, vec![pay(vec![resource(1, 0)], body(2, vec![]))]));
    let alternatives = |second| {
        control(Control::Pay {
            at: PaymentTime::ResolutionChoice,
            costs: CostSpec::Alternative {
                alternatives: vec![
                    CostSpec::Ordered {
                        steps: vec![resource(1, 1)],
                    },
                    CostSpec::Ordered {
                        steps: vec![resource(2, second)],
                    },
                ],
            },
            optional: PaymentOptionality::Optional,
            on_paid: body(2, vec![]),
            receipt: ReceiptId(9),
        })
    };
    accept(body(1, vec![alternatives(2)]));
    reject(
        body(1, vec![alternatives(-1)]),
        WellFormedReason::UnsupportedShape,
    );
    let mut sibling = resource(2, 1);
    sibling.instruction.action = Action::Counter {
        holder: Holder::Player {
            player: PlayerRef::Controller {},
        },
        kind: CounterKind("stack".into()),
        operation: CounterOperation::Remove {},
        amount: actual(ReceiptReference::new(ReceiptId(1))),
    };
    reject(
        body(
            1,
            vec![control(Control::Pay {
                at: PaymentTime::ResolutionChoice,
                costs: CostSpec::Alternative {
                    alternatives: vec![
                        CostSpec::Ordered {
                            steps: vec![cost(DiscardMode::DownTo { count: Nat(2) }, None)],
                        },
                        CostSpec::Ordered {
                            steps: vec![sibling],
                        },
                    ],
                },
                optional: PaymentOptionality::Required,
                on_paid: body(2, vec![]),
                receipt: ReceiptId(9),
            })],
        ),
        WellFormedReason::UnavailableReceipt,
    );
}

#[test]
fn costs_cannot_be_conditionally_skipped_or_publish_materials_without_selection() {
    let mut guarded = resource(1, 1);
    guarded.instruction.precondition = Some(Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        predicate: Expr::Literal(Bool(false)),
        protects: InstructionId(1),
        reason: PreconditionReason::CardText {},
    });
    reject(
        body(1, vec![pay(vec![guarded], body(2, vec![]))]),
        WellFormedReason::PreconditionTiming,
    );
    let mut materials = resource(1, 1);
    materials.materials = Some(Binding::new(1, ScopeId(2)));
    reject(
        body(1, vec![pay(vec![materials], body(2, vec![]))]),
        WellFormedReason::UnsupportedShape,
    );
    let mut life = resource(1, 1);
    life.instruction.action = Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Literal(Int(-2)),
        },
    };
    accept(body(1, vec![pay(vec![life.clone()], body(2, vec![]))]));
    life.instruction.action = Action::Life {
        player: PlayerRef::Controller {},
        change: NumericChange::Add {
            amount: Expr::Literal(Int(2)),
        },
    };
    reject(
        body(1, vec![pay(vec![life], body(2, vec![]))]),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn play_costs_and_optional_play_payment_have_an_explicit_entry() {
    let make = |steps| {
        let mut program = card(body(1, steps), "BP20-P12").node;
        program.abilities[0].node.play = Some(PlaySpec {
            limit: None,
            captures: vec![],
            parameters: vec![],
            costs: CostSpec::Ordered {
                steps: vec![resource(1, 2)],
            },
        });
        program
    };
    let optional = control(Control::Pay {
        at: PaymentTime::PlayParameter,
        costs: CostSpec::Ordered {
            steps: vec![resource(2, 1)],
        },
        optional: PaymentOptionality::Optional,
        on_paid: body(2, vec![]),
        receipt: ReceiptId(9),
    });
    let result = control(Control::If {
        condition: complete(ReceiptReference::new(ReceiptId(9))),
        then_body: body(3, vec![draw(3, Expr::Literal(Int(1)))]),
        else_body: None,
    });
    WellFormedCardProgram::check(make(vec![optional.clone(), result])).unwrap();
    let late = make(vec![draw(3, Expr::Literal(Int(1))), optional.clone()]);
    assert_eq!(
        WellFormedCardProgram::check(late).unwrap_err().reason,
        WellFormedReason::DecisionTiming
    );
    reject(body(1, vec![optional]), WellFormedReason::DecisionTiming);
    let mut invalid = make(vec![]);
    if let CostSpec::Ordered { steps } = &mut invalid.abilities[0].node.play.as_mut().unwrap().costs
    {
        steps[0].instruction.context = InstructionContext::Effect;
    }
    assert_eq!(
        WellFormedCardProgram::check(invalid).unwrap_err().reason,
        WellFormedReason::ExpectedCost
    );
}
