use super::*;
mod action_boundaries;
mod evolution;

fn bounds(minimum: u64, maximum: Option<u64>) -> Cardinality {
    Cardinality {
        minimum: Nat(minimum),
        maximum: maximum.map(Nat),
    }
}
fn zone(player: PlayerRef, kind: ZoneKind, candidate: u64) -> Selector {
    Selector::Zone {
        players: PlayerSet {
            relative_to: player,
            ..controller_set()
        },
        zone: kind,
        view: SelectionView::Current {},
        binding: Binding::new(candidate, ScopeId(candidate.checked_add(100).unwrap())),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}
fn constraint(selector: Selector) -> SelectionConstraint {
    SelectionConstraint {
        groups: vec![SelectionGroup {
            selector,
            cardinality: bounds(0, Some(1)),
        }],
        cardinality: bounds(0, Some(1)),
        distinct_by: None,
        disjoint: Disjointness::Disjoint,
        aggregate_bounds: vec![],
    }
}
fn choose(id: u64, scope: u64) -> Decision {
    Decision {
        captures: vec![],
        id: DecisionId(id),
        actor: PlayerRef::Controller {},
        domain: ChoiceDomain::Integer {
            cardinality: bounds(0, Some(3)),
            binding: Binding::new(id, ScopeId(scope)),
        },
        kind: DecisionKind::ResolutionChoice {
            may: Declinable::Required,
            cardinality: bounds(0, Some(3)),
            precondition: None,
        },
        visibility: Visibility::Public {},
    }
}
fn decision(decision: Decision) -> Located<Node> {
    Located {
        source: source("synthetic"),
        node: Node::Decision { decision },
    }
}
fn chosen_objects(id: u64, scope: u64, selector: Selector) -> Decision {
    Decision {
        domain: ChoiceDomain::Objects {
            constraint: constraint(selector),
            binding: Binding::new(id, ScopeId(scope)),
            target_roles: None,
        },
        kind: DecisionKind::ResolutionChoice {
            may: Declinable::Required,
            cardinality: bounds(0, Some(1)),
            precondition: None,
        },
        ..choose(id, scope)
    }
}
fn discard(mode: DiscardMode) -> Located<Node> {
    instruction(
        Action::Discard {
            player: PlayerRef::Controller {},
            mode,
        },
        1,
        "BP10-076",
    )
}
fn hand() -> Selector {
    zone(PlayerRef::Controller {}, ZoneKind::Hand, 10)
}
fn cost(mode: DiscardMode, materials: Option<Binding<Selector>>) -> CostStep {
    let Node::Instruction { mut instruction } = discard(mode).node else {
        panic!("instruction")
    };
    instruction.context = InstructionContext::Cost;
    CostStep {
        instruction,
        materials,
    }
}
fn pay(steps: Vec<CostStep>, on_paid: Program) -> Located<Node> {
    control(Control::Pay {
        at: PaymentTime::ResolutionChoice,
        costs: CostSpec::Ordered { steps },
        optional: PaymentOptionality::Optional,
        on_paid,
        receipt: ReceiptId(9),
    })
}
fn complete(receipt: ReceiptReference<PaymentResult>) -> Expr<Bool> {
    Expr::Frozen(Box::new(BoolFrozen::PaymentComplete {
        receipt: PaymentReference::Local { receipt },
    }))
}

#[test]
fn resolution_answers_are_typed_unique_and_dominate_uses() {
    let answer = Binding::new(1, ScopeId(1));
    accept(body(
        1,
        vec![decision(choose(1, 1)), draw(2, signed(Expr::Bound(answer)))],
    ));
    reject(
        body(
            1,
            vec![draw(2, signed(Expr::Bound(answer))), decision(choose(1, 1))],
        ),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(
            1,
            vec![
                decision(choose(1, 1)),
                draw(2, Expr::Bound(Binding::new(1, ScopeId(1)))),
            ],
        ),
        WellFormedReason::BindingType,
    );
    reject(
        body(
            1,
            vec![
                decision(choose(1, 1)),
                control(Control::Sequence {
                    body: body(2, vec![decision(choose(1, 2))]),
                }),
            ],
        ),
        WellFormedReason::DuplicateDecision,
    );
    let mut invalid_1 = choose(1, 1);
    invalid_1.domain = ChoiceDomain::Integer {
        cardinality: bounds(0, None),
        binding: answer,
    };
    reject(
        body(1, vec![decision(invalid_1)]),
        WellFormedReason::DecisionCardinality,
    );
    let mut invalid_2 = choose(1, 1);
    invalid_2.kind = DecisionKind::QuickWindow {
        window: QuickWindow::Attack,
    };
    reject(
        body(1, vec![decision(invalid_2)]),
        WellFormedReason::DecisionTiming,
    );
    let mut decline = choose(1, 1);
    if let DecisionKind::ResolutionChoice { may, .. } = &mut decline.kind {
        *may = Declinable::MayDecline;
    }
    reject(
        body(1, vec![decision(decline)]),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn protected_decision_and_instruction_share_one_start_and_no_answer_escapes() {
    let guard = Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        predicate: Expr::Literal(Bool(false)),
        protects: InstructionId(2),
        reason: PreconditionReason::CardText {},
    };
    let mut selection = choose(1, 1);
    if let DecisionKind::ResolutionChoice { precondition, .. } = &mut selection.kind {
        *precondition = Some(guard.clone());
    }
    let mut action = draw(2, signed(Expr::Bound(Binding::new(1, ScopeId(1)))));
    if let Node::Instruction { instruction } = &mut action.node {
        instruction.precondition = Some(guard);
    }
    accept(body(
        1,
        vec![
            decision(selection.clone()),
            action.clone(),
            draw(3, signed(actual(ReceiptReference::new(ReceiptId(2))))),
        ],
    ));
    reject(
        body(1, vec![decision(selection.clone())]),
        WellFormedReason::PreconditionTiming,
    );
    reject(
        body(
            1,
            vec![
                decision(selection.clone()),
                draw(3, Expr::Literal(Int(0))),
                action.clone(),
            ],
        ),
        WellFormedReason::PreconditionTiming,
    );
    reject(
        body(
            1,
            vec![
                decision(selection.clone()),
                control(Control::Sequence {
                    body: body(2, vec![action.clone()]),
                }),
            ],
        ),
        WellFormedReason::PreconditionTiming,
    );
    reject(
        body(
            1,
            vec![
                decision(selection.clone()),
                action.clone(),
                draw(3, signed(Expr::Bound(Binding::new(1, ScopeId(1))))),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
    if let Node::Instruction { instruction } = &mut action.node {
        instruction.precondition.as_mut().unwrap().predicate = Expr::Literal(Bool(true));
    }
    reject(
        body(1, vec![decision(selection.clone()), action]),
        WellFormedReason::PreconditionTiming,
    );
    if let DecisionKind::ResolutionChoice {
        precondition: Some(self_guard),
        ..
    } = &mut selection.kind
    {
        self_guard.predicate = positive(Expr::Bound(Binding::new(1, ScopeId(1))));
    }
    reject(
        body(1, vec![decision(selection)]),
        WellFormedReason::UnavailableBinding,
    );
}

#[test]
fn discard_modes_use_only_the_holders_hand_and_one_cost_selection() {
    let modes = [
        DiscardMode::Chosen {
            constraint: constraint(hand()),
        },
        DiscardMode::Random {
            population: hand(),
            count: Expr::Literal(Int(1)),
        },
        DiscardMode::All { eligible: hand() },
        DiscardMode::DownTo { count: Nat(2) },
    ];
    for mode in modes {
        accept(body(1, vec![discard(mode.clone())]));
        let materials = Binding::new(20, ScopeId(2));
        accept(body(
            1,
            vec![pay(
                vec![cost(mode.clone(), Some(materials))],
                body(
                    2,
                    vec![instruction(
                        Action::Reveal {
                            objects: Selector::Bound {
                                reference: materials,
                            },
                        },
                        2,
                        "synthetic",
                    )],
                ),
            )],
        ));
        reject(
            body(
                1,
                vec![
                    pay(vec![cost(mode, Some(materials))], body(2, vec![])),
                    instruction(
                        Action::Reveal {
                            objects: Selector::Bound {
                                reference: materials,
                            },
                        },
                        3,
                        "synthetic",
                    ),
                ],
            ),
            WellFormedReason::UnavailableBinding,
        );
    }
    for kind in [ZoneKind::Field, ZoneKind::Cemetery] {
        reject(
            body(
                1,
                vec![discard(DiscardMode::All {
                    eligible: zone(PlayerRef::Controller {}, kind, 10),
                })],
            ),
            WellFormedReason::InvalidDiscardDomain,
        );
    }
    reject(
        body(
            1,
            vec![discard(DiscardMode::Chosen {
                constraint: constraint(zone(PlayerRef::Owner {}, ZoneKind::Hand, 10)),
            })],
        ),
        WellFormedReason::UnsupportedShape,
    );
    let mut opponent = discard(DiscardMode::DownTo { count: Nat(2) });
    if let Node::Instruction { instruction } = &mut opponent.node {
        instruction.actor = PlayerRef::Owner {};
    }
    reject(body(1, vec![opponent]), WellFormedReason::UnsupportedShape);
    let mut any = constraint(hand());
    any.cardinality = bounds(0, None);
    any.groups[0].cardinality = bounds(0, None);
    accept(body(
        1,
        vec![discard(DiscardMode::Chosen {
            constraint: any.clone(),
        })],
    ));
    any.disjoint = Disjointness::MayOverlap;
    reject(
        body(1, vec![discard(DiscardMode::Chosen { constraint: any })]),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn private_domains_cannot_expose_hidden_candidates_or_filter_other_hands() {
    let mut selection = chosen_objects(1, 1, hand());
    reject(
        body(1, vec![decision(selection.clone())]),
        WellFormedReason::UnsupportedShape,
    );
    selection.visibility = Visibility::Only {
        players: controller_set(),
    };
    accept(body(1, vec![decision(selection.clone())]));
    selection.actor = PlayerRef::Owner {};
    reject(
        body(1, vec![decision(selection)]),
        WellFormedReason::UnsupportedShape,
    );
    let mut selector = hand();
    if let Selector::Zone { predicate, .. } = &mut selector {
        **predicate = Expr::Compute(Box::new(Predicate::Exists {
            objects: zone(PlayerRef::Owner {}, ZoneKind::Hand, 11),
        }));
    }
    reject(
        body(1, vec![discard(DiscardMode::All { eligible: selector })]),
        WellFormedReason::UnsupportedShape,
    );
    reject(
        body(
            1,
            vec![discard(DiscardMode::Random {
                population: zone(PlayerRef::Controller {}, ZoneKind::Deck, 10),
                count: Expr::Literal(Int(1)),
            })],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn payments_publish_complete_after_costs_and_keep_locals_inside_on_paid() {
    let payment = || {
        pay(
            vec![cost(DiscardMode::DownTo { count: Nat(2) }, None)],
            body(
                2,
                vec![control(Control::If {
                    condition: complete(ReceiptReference::new(ReceiptId(9))),
                    then_body: body(3, vec![]),
                    else_body: None,
                })],
            ),
        )
    };
    accept(body(
        1,
        vec![
            payment(),
            control(Control::If {
                condition: complete(ReceiptReference::new(ReceiptId(9))),
                then_body: body(4, vec![]),
                else_body: None,
            }),
        ],
    ));
    let mut early = cost(DiscardMode::DownTo { count: Nat(2) }, None);
    early.instruction.precondition = Some(Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        predicate: complete(ReceiptReference::new(ReceiptId(9))),
        protects: InstructionId(1),
        reason: PreconditionReason::CardText {},
    });
    reject(
        body(1, vec![pay(vec![early], body(2, vec![]))]),
        WellFormedReason::PreconditionTiming,
    );
    let mut invalid_3 = cost(DiscardMode::DownTo { count: Nat(2) }, None);
    invalid_3.instruction.context = InstructionContext::Effect;
    reject(
        body(1, vec![pay(vec![invalid_3], body(2, vec![]))]),
        WellFormedReason::ExpectedCost,
    );
    let mut invalid_4 = payment();
    if let Node::Control {
        control: Control::Pay { at, .. },
    } = &mut invalid_4.node
    {
        *at = PaymentTime::PlayParameter;
    }
    reject(body(1, vec![invalid_4]), WellFormedReason::DecisionTiming);
    let mut invalid_5 = payment();
    if let Node::Control {
        control: Control::Pay { receipt, .. },
    } = &mut invalid_5.node
    {
        *receipt = ReceiptId(1);
    }
    reject(body(1, vec![invalid_5]), WellFormedReason::DuplicateReceipt);
    let mut invalid_6 = cost(DiscardMode::DownTo { count: Nat(2) }, None);
    invalid_6.instruction.action = Action::Draw {
        player: PlayerRef::Controller {},
        count: Expr::Literal(Int(1)),
        mode: StepwiseMode::Exact,
    };
    reject(
        body(1, vec![pay(vec![invalid_6], body(2, vec![]))]),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn repeat_payment_collection_retains_typed_producers_and_allows_alias_exports() {
    let payment = pay(
        vec![cost(DiscardMode::DownTo { count: Nat(2) }, None)],
        body(3, vec![]),
    );
    let collected = Binding::new(1, ScopeId(1));
    let exports = vec![
        ReceiptExport::Payment {
            source: ReceiptReference::new(ReceiptId(9)),
            iteration: Binding::new(1, ScopeId(4)),
            collect: Some(collected),
        },
        ReceiptExport::Payment {
            source: ReceiptReference::new(ReceiptId(9)),
            iteration: Binding::new(2, ScopeId(4)),
            collect: None,
        },
    ];
    let repeat = control(Control::Repeat {
        body: RepeatBody::Program {
            body: body(2, vec![payment]),
        },
        termination: Termination::Until {
            predicate: complete(ReceiptReference::iteration(Binding::new(1, ScopeId(4)))),
        },
        check: IterationCheck::AfterIteration,
        scope: ScopeId(4),
        receipts: exports,
    });
    accept(body(
        1,
        vec![
            repeat.clone(),
            control(Control::If {
                condition: Expr::Frozen(Box::new(BoolFrozen::PaymentsComplete {
                    receipts: collected,
                    quantifier: ReceiptQuantifier::All,
                })),
                then_body: body(5, vec![]),
                else_body: None,
            }),
        ],
    ));
    reject(
        body(
            1,
            vec![
                repeat,
                draw(
                    3,
                    signed(Expr::Compute(Box::new(NatOperation::ReceiptActualSum {
                        receipts: MeasuredReceiptList::Cards {
                            receipts: Binding::new(1, ScopeId(1)),
                        },
                    }))),
                ),
            ],
        ),
        WellFormedReason::BindingType,
    );
}

#[test]
fn batch_choices_precede_commit_without_sibling_dependencies() {
    let chosen = Binding::new(1, ScopeId(1));
    let mut batch = simultaneous(vec![draw(1, signed(Expr::Bound(chosen)))], 3);
    if let Node::Control {
        control: Control::Simultaneous { batch, .. },
    } = &mut batch.node
    {
        batch.choice_stages = vec![BatchChoiceStage {
            decisions: vec![choose(1, 1)],
            order: PlayerOrder::TurnPlayerFirst,
        }];
    }
    accept(body(1, vec![batch.clone()]));
    let mut dependent = choose(2, 1);
    dependent.actor = PlayerRef::Bound {
        binding: Binding::new(1, ScopeId(1)),
    };
    if let Node::Control {
        control: Control::Simultaneous { batch, .. },
    } = &mut batch.node
    {
        batch.choice_stages[0].decisions.push(dependent);
    }
    reject(body(1, vec![batch]), WellFormedReason::UnavailableBinding);
}

#[test]
fn batch_selections_are_local_but_member_receipts_are_committed() {
    let chosen = Binding::new(1, ScopeId(1));
    let mut batch = simultaneous(vec![draw(1, signed(Expr::Bound(chosen)))], 3);
    if let Node::Control {
        control: Control::Simultaneous { batch, .. },
    } = &mut batch.node
    {
        batch.choice_stages = vec![BatchChoiceStage {
            decisions: vec![choose(1, 1)],
            order: PlayerOrder::TurnPlayerFirst,
        }];
    }
    accept(body(
        1,
        vec![
            batch.clone(),
            draw(4, signed(actual(ReceiptReference::new(ReceiptId(1))))),
        ],
    ));
    reject(
        body(1, vec![batch.clone(), draw(4, signed(Expr::Bound(chosen)))]),
        WellFormedReason::UnavailableBinding,
    );
    if let Node::Control {
        control: Control::Simultaneous { batch, .. },
    } = &mut batch.node
        && let DecisionKind::ResolutionChoice { precondition, .. } =
            &mut batch.choice_stages[0].decisions[0].kind
    {
        *precondition = Some(Precondition {
            kind: PreconditionKind::CardTextPrerequisite,
            predicate: Expr::Literal(Bool(true)),
            protects: InstructionId(1),
            reason: PreconditionReason::CardText {},
        });
    }
    reject(body(1, vec![batch]), WellFormedReason::PreconditionTiming);
}

#[test]
fn a_capture_cannot_launder_unverified_private_information_into_a_domain() {
    let captured = Binding::new(20, ScopeId(1));
    let mut selection = constraint(hand());
    selection.aggregate_bounds.push(AggregateBound {
        property: NumericProperty::Cost,
        comparison: Comparison::LessEqual,
        value: Expr::Captured(CaptureRef { binding: captured }),
    });
    reject(
        body(
            1,
            vec![
                capture(
                    captured,
                    signed(Expr::Compute(Box::new(NatOperation::Count {
                        objects: zone(PlayerRef::Owner {}, ZoneKind::Hand, 77),
                    }))),
                ),
                discard(DiscardMode::Chosen {
                    constraint: selection,
                }),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

mod review_regressions;

mod entries;

mod producers;

mod observations;

mod payments;

mod movement;
mod player_cards;

mod modification;
