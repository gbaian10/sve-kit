use super::*;

fn recipients() -> RecipientSet {
    RecipientSet::Leaders {
        players: controller_set(),
    }
}
fn damage(id: u64, recipients: RecipientSet, amount: DamageAmount) -> Located<Node> {
    instruction(
        Action::Damage {
            recipients,
            amount,
            flags: DamageFlags::Ability,
            source: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            batch: BatchPolicy::Instruction {},
        },
        id,
        "BP02-052",
    )
}
fn distribution(id: u64, recipients: RecipientSet) -> Decision {
    Decision {
        domain: ChoiceDomain::Distribution {
            minimum: Nat(0),
            recipients,
            total: Expr::Literal(Nat(3)),
            binding: Binding::new(id, ScopeId(1)),
        },
        kind: DecisionKind::ResolutionChoice {
            may: Declinable::Required,
            cardinality: bounds(1, Some(1)),
            precondition: None,
        },
        ..choose(id, 1)
    }
}
fn token(id: u64, scope: u64) -> Located<Node> {
    instruction(
        Action::Token {
            tokens: vec![TokenSpec {
                template: TokenTemplateId("synthetic-fairy".into()),
                name: Expr::Literal(CardName("フェアリー".into())),
                count: Expr::Literal(Nat(3)),
            }],
            destination: TokenDestination::Single {
                zone: Expr::Literal(ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                }),
            },
            output: Binding::new(id, ScopeId(scope)),
            initial_state: None,
            initial_posture: Posture::Standing,
            entry_effects: vec![],
        },
        id,
        "SD01-006",
    )
}

#[test]
fn guarded_producers_keep_receipts_but_cannot_publish_conditional_bindings() {
    let integer = Binding::new(1, ScopeId(1));
    let natural = Binding::new(1, ScopeId(1));
    let objects = Binding::new(1, ScopeId(1));
    let cases = [
        (declare(1, integer), draw(2, Expr::Bound(integer))),
        (
            instruction(
                Action::Dice {
                    operation: DiceOperation::Roll {},
                    result: natural,
                },
                1,
                "BP06-110",
            ),
            draw(2, signed(Expr::Bound(natural))),
        ),
        (
            token(1, 1),
            instruction(
                Action::Reveal {
                    objects: Selector::Bound { reference: objects },
                },
                2,
                "synthetic",
            ),
        ),
    ];
    for (producer, consumer) in cases {
        accept(body(1, vec![producer.clone(), consumer.clone()]));
        let mut guarded = producer;
        if let Node::Instruction { instruction } = &mut guarded.node {
            instruction.precondition = Some(Precondition {
                kind: PreconditionKind::CardTextPrerequisite,
                predicate: Expr::Literal(Bool(false)),
                protects: instruction.id,
                reason: PreconditionReason::CardText {},
            });
        }
        reject(
            body(1, vec![guarded.clone(), consumer.clone()]),
            WellFormedReason::UnavailableBinding,
        );
        if !matches!(&guarded.node, Node::Instruction { instruction }
            if matches!(instruction.action, Action::Declare { .. }))
        {
            reject(
                body(1, vec![simultaneous(vec![guarded.clone()], 3), consumer]),
                WellFormedReason::UnavailableBinding,
            );
        }
        if matches!(&guarded.node, Node::Instruction { instruction }
            if matches!(instruction.action, Action::Token { .. }))
        {
            accept(body(
                1,
                vec![
                    guarded,
                    instruction(
                        Action::Reveal {
                            objects: Selector::Created {
                                receipt: ReceiptReference::new(ReceiptId(1)),
                            },
                        },
                        2,
                        "synthetic",
                    ),
                ],
            ));
        }
    }
}

#[test]
fn allocation_requires_its_exact_dominating_domain() {
    let allocation = DamageAmount::Allocation {
        binding: Binding::new(1, ScopeId(1)),
    };
    accept(body(
        1,
        vec![
            decision(distribution(1, recipients())),
            damage(2, recipients(), allocation.clone()),
        ],
    ));
    reject(
        body(1, vec![damage(2, recipients(), allocation.clone())]),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            1,
            vec![
                decision(distribution(1, recipients())),
                damage(
                    2,
                    RecipientSet::Explicit { recipients: vec![] },
                    allocation.clone(),
                ),
            ],
        ),
        WellFormedReason::DistributionRecipients,
    );
    reject(
        body(
            1,
            vec![decision(choose(1, 1)), damage(2, recipients(), allocation)],
        ),
        WellFormedReason::BindingType,
    );
    let mut invalid = distribution(1, recipients());
    invalid.kind = DecisionKind::ResolutionChoice {
        may: Declinable::Required,
        cardinality: bounds(0, Some(3)),
        precondition: None,
    };
    reject(
        body(1, vec![decision(invalid)]),
        WellFormedReason::DecisionCardinality,
    );
}

#[test]
fn recipient_sets_accept_empty_unions_and_check_every_dynamic_member() {
    let union = RecipientSet::Union {
        sets: vec![
            recipients(),
            recipients(),
            RecipientSet::Explicit { recipients: vec![] },
        ],
    };
    accept(body(
        1,
        vec![damage(
            1,
            union,
            DamageAmount::Uniform {
                amount: Expr::Literal(Int(3)),
            },
        )],
    ));
    accept(body(
        1,
        vec![damage(
            1,
            RecipientSet::Union { sets: vec![] },
            DamageAmount::Uniform {
                amount: Expr::Literal(Int(0)),
            },
        )],
    ));
    accept(body(
        1,
        vec![damage(
            1,
            RecipientSet::Objects {
                objects: zone(PlayerRef::Controller {}, ZoneKind::Field, 4),
            },
            DamageAmount::Uniform {
                amount: Expr::Literal(Int(-2)),
            },
        )],
    ));
    for set in [
        RecipientSet::Bound {
            recipients: Binding::new(9, ScopeId(1)),
        },
        RecipientSet::Explicit {
            recipients: vec![DamageRecipient::Object {
                object: ObjectOperand::Bound {
                    binding: Binding::new(9, ScopeId(1)),
                },
            }],
        },
    ] {
        reject(
            body(
                1,
                vec![damage(
                    1,
                    set,
                    DamageAmount::Uniform {
                        amount: Expr::Literal(Int(1)),
                    },
                )],
            ),
            WellFormedReason::UnavailableBinding,
        );
    }
    let dynamic = RecipientSet::Objects {
        objects: zone(PlayerRef::Controller {}, ZoneKind::Field, 4),
    };
    accept(body(
        1,
        vec![
            decision(distribution(1, dynamic.clone())),
            damage(
                2,
                dynamic,
                DamageAmount::Allocation {
                    binding: Binding::new(1, ScopeId(1)),
                },
            ),
        ],
    ));
}

#[test]
fn damage_and_token_producers_export_only_their_own_typed_receipts() {
    let collected = Binding::new(4, ScopeId(1));
    let repeat = control(Control::Repeat {
        body: RepeatBody::Program {
            body: body(
                2,
                vec![
                    damage(
                        1,
                        recipients(),
                        DamageAmount::Uniform {
                            amount: Expr::Literal(Int(1)),
                        },
                    ),
                    token(2, 2),
                ],
            ),
        },
        termination: Termination::Count {
            count: Expr::Literal(Nat(2)),
        },
        check: IterationCheck::BeforeIteration,
        scope: ScopeId(3),
        receipts: vec![
            ReceiptExport::DamageQuantity {
                source: ReceiptReference::new(ReceiptId(1)),
                iteration: Binding::new(1, ScopeId(3)),
                collect: Some(collected),
            },
            ReceiptExport::Token {
                source: ReceiptReference::new(ReceiptId(2)),
                iteration: Binding::new(2, ScopeId(3)),
                collect: Some(Binding::new(5, ScopeId(1))),
            },
        ],
    });
    accept(body(
        1,
        vec![
            repeat.clone(),
            draw(
                3,
                signed(Expr::Compute(Box::new(NatOperation::ReceiptActualSum {
                    receipts: MeasuredReceiptList::Damage {
                        receipts: collected,
                    },
                }))),
            ),
        ],
    ));
    let mut invalid = repeat;
    if let Node::Control {
        control: Control::Repeat { receipts, .. },
    } = &mut invalid.node
    {
        receipts[0] = ReceiptExport::DamageQuantity {
            source: ReceiptReference::new(ReceiptId(2)),
            iteration: Binding::new(1, ScopeId(3)),
            collect: None,
        };
    }
    reject(body(1, vec![invalid]), WellFormedReason::ReceiptType);
    accept(body(
        1,
        vec![
            token(1, 1),
            instruction(
                Action::Reveal {
                    objects: Selector::Created {
                        receipt: ReceiptReference::new(ReceiptId(1)),
                    },
                },
                2,
                "SD01-006",
            ),
        ],
    ));
    reject(
        body(
            1,
            vec![
                token(1, 1),
                draw(2, signed(actual(ReceiptReference::new(ReceiptId(1))))),
            ],
        ),
        WellFormedReason::ReceiptType,
    );
}

#[test]
fn token_inputs_are_checked_before_output_and_siblings_share_no_outputs() {
    let mut own = token(1, 1);
    if let Node::Instruction { instruction } = &mut own.node
        && let Action::Token { tokens, .. } = &mut instruction.action
    {
        tokens[0].count = Expr::Compute(Box::new(NatOperation::Count {
            objects: Selector::Bound {
                reference: Binding::new(1, ScopeId(1)),
            },
        }));
    }
    reject(body(1, vec![own]), WellFormedReason::UnavailableBinding);
    accept(body(
        1,
        vec![simultaneous(vec![token(1, 1), token(2, 1)], 3)],
    ));
    reject(
        body(
            1,
            vec![simultaneous(
                vec![
                    token(1, 1),
                    instruction(
                        Action::Reveal {
                            objects: Selector::Created {
                                receipt: ReceiptReference::new(ReceiptId(1)),
                            },
                        },
                        2,
                        "synthetic",
                    ),
                ],
                3,
            )],
        ),
        WellFormedReason::UnavailableReceipt,
    );
    let mut invalid = token(1, 1);
    if let Node::Instruction { instruction } = &mut invalid.node
        && let Action::Token { destination, .. } = &mut instruction.action
    {
        *destination = TokenDestination::Single {
            zone: Expr::Literal(ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            }),
        };
    }
    reject(body(1, vec![invalid]), WellFormedReason::UnsupportedShape);
}
