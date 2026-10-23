use super::*;

fn private(actor: PlayerRef) -> Visibility {
    Visibility::Only {
        players: PlayerSet {
            relative_to: actor,
            ..controller_set()
        },
    }
}
fn look_into(id: u64) -> Located<Node> {
    instruction(
        Action::Look {
            viewer: PlayerRef::Controller {},
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(3)),
            mode: StepwiseMode::Exact,
        },
        id,
        "synthetic",
    )
}
fn looked_at(id: u64) -> Selector {
    Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look {
            receipt: ReceiptReference::new(ReceiptId(id)),
        },
        projection: MemberProjection::After,
    }
}
fn look() -> Located<Node> {
    look_into(1)
}
fn looked() -> Selector {
    looked_at(1)
}

#[test]
fn look_receipt_is_visible_only_to_viewer_until_revealed() {
    let mut choice = chosen_objects(2, 1, looked());
    choice.visibility = private(PlayerRef::Controller {});
    accept(body(1, vec![look(), decision(choice.clone())]));
    reject(
        body(1, vec![decision(choice.clone()), look()]),
        WellFormedReason::UnsupportedShape,
    );
    let mut opponent = choice.clone();
    opponent.actor = PlayerRef::Owner {};
    opponent.visibility = private(PlayerRef::Owner {});
    reject(
        body(1, vec![look(), decision(opponent)]),
        WellFormedReason::UnsupportedShape,
    );
    choice.visibility = Visibility::Public {};
    reject(
        body(1, vec![look(), decision(choice.clone())]),
        WellFormedReason::UnsupportedShape,
    );
    accept(body(
        1,
        vec![
            look(),
            instruction(Action::Reveal { objects: looked() }, 3, "synthetic"),
            decision(choice),
        ],
    ));
}

#[test]
fn private_answer_and_selection_capture_keep_their_audience() {
    let selected = Binding::new(2, ScopeId(1));
    let captured = Binding::new(3, ScopeId(1));
    let mut choice = chosen_objects(2, 1, looked());
    choice.visibility = private(PlayerRef::Controller {});
    choice.captures.push(Capture::Objects {
        at: CaptureTime::AtSelection {
            decision: DecisionId(2),
        },
        value: Selector::Bound {
            reference: selected,
        },
        into: CaptureRef { binding: captured },
    });
    let subsequent = |objects| {
        let mut second = chosen_objects(4, 1, objects);
        second.visibility = private(PlayerRef::Controller {});
        second
    };
    for objects in [
        Selector::Bound {
            reference: selected,
        },
        Selector::Captured {
            reference: CaptureRef { binding: captured },
        },
    ] {
        accept(body(
            1,
            vec![
                look(),
                decision(choice.clone()),
                decision(subsequent(objects.clone())),
            ],
        ));
        let mut public = subsequent(objects);
        public.visibility = Visibility::Public {};
        reject(
            body(1, vec![look(), decision(choice.clone()), decision(public)]),
            WellFormedReason::UnsupportedShape,
        );
    }
    let mut public = subsequent(Selector::Bound {
        reference: selected,
    });
    public.visibility = Visibility::Public {};
    accept(body(
        1,
        vec![
            look(),
            decision(choice),
            instruction(
                Action::Reveal {
                    objects: Selector::Bound {
                        reference: selected,
                    },
                },
                5,
                "synthetic",
            ),
            decision(public),
        ],
    ));
}

#[test]
fn scalar_capture_propagates_private_dependencies_without_laundering() {
    let captured = Binding::new(20, ScopeId(1));
    let mut selection = constraint(hand());
    selection.aggregate_bounds.push(AggregateBound {
        property: NumericProperty::Cost,
        comparison: Comparison::LessEqual,
        value: Expr::Captured(CaptureRef { binding: captured }),
    });
    accept(body(
        1,
        vec![
            capture(captured, Expr::Literal(Int(1))),
            discard(DiscardMode::Chosen {
                constraint: selection.clone(),
            }),
        ],
    ));
    let count = signed(Expr::Compute(Box::new(NatOperation::Count {
        objects: zone(PlayerRef::Controller {}, ZoneKind::Hand, 70),
    })));
    accept(body(
        1,
        vec![
            capture(captured, count.clone()),
            discard(DiscardMode::Chosen {
                constraint: selection,
            }),
        ],
    ));
    let mut public = chosen_objects(2, 1, zone(PlayerRef::Controller {}, ZoneKind::Field, 71));
    if let ChoiceDomain::Objects { constraint, .. } = &mut public.domain {
        constraint.aggregate_bounds.push(AggregateBound {
            property: NumericProperty::Cost,
            comparison: Comparison::LessEqual,
            value: Expr::Captured(CaptureRef { binding: captured }),
        });
    }
    reject(
        body(1, vec![capture(captured, count), decision(public)]),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn chosen_materials_are_consumed_once_and_still_require_current_hand_membership() {
    let selected = Binding::new(2, ScopeId(1));
    let mut choose_hand = chosen_objects(2, 1, hand());
    choose_hand.visibility = private(PlayerRef::Controller {});
    let consumed = Selector::Intersection {
        sets: vec![
            zone(PlayerRef::Controller {}, ZoneKind::Hand, 11),
            Selector::Bound {
                reference: selected,
            },
        ],
    };
    accept(body(
        1,
        vec![
            decision(choose_hand.clone()),
            discard(DiscardMode::All { eligible: consumed }),
        ],
    ));
    reject(
        body(
            1,
            vec![
                decision(choose_hand),
                discard(DiscardMode::All {
                    eligible: Selector::Bound {
                        reference: selected,
                    },
                }),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

fn start_guard(protects: u64) -> Precondition {
    Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        predicate: Expr::Literal(Bool(false)),
        protects: InstructionId(protects),
        reason: PreconditionReason::CardText {},
    }
}
fn guarded_choice(id: u64, protects: u64) -> Located<Node> {
    let mut choice = choose(id, 1);
    if let DecisionKind::ResolutionChoice { precondition, .. } = &mut choice.kind {
        *precondition = Some(start_guard(protects));
    }
    decision(choice)
}
fn guarded(mut node: Located<Node>, protects: u64) -> Located<Node> {
    if let Node::Instruction { instruction } = &mut node.node {
        instruction.precondition = Some(start_guard(protects));
    }
    node
}
fn reveal(id: u64, objects: Selector) -> Located<Node> {
    instruction(Action::Reveal { objects }, id, "synthetic")
}
fn flip(id: u64, objects: Selector) -> Located<Node> {
    instruction(Action::FlipDown { objects }, id, "synthetic")
}
fn public_choice(id: u64, scope: u64, objects: Selector) -> Located<Node> {
    decision(chosen_objects(id, scope, objects))
}
fn source_holder() -> Selector {
    Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    }
}

#[test]
fn a_reveal_behind_a_start_guard_never_publishes_its_objects() {
    accept(body(
        1,
        vec![look(), reveal(3, looked()), public_choice(4, 1, looked())],
    ));
    accept(body(
        1,
        vec![
            look(),
            simultaneous(vec![reveal(3, looked())], 10),
            public_choice(4, 1, looked()),
        ],
    ));
    // A false start guard skips both the guarded answer and the reveal it protects.
    reject(
        body(
            1,
            vec![
                look(),
                guarded_choice(2, 3),
                guarded(reveal(3, looked()), 3),
                public_choice(4, 1, looked()),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
    reject(
        body(
            1,
            vec![
                look(),
                simultaneous(vec![guarded(reveal(3, looked()), 3)], 10),
                public_choice(4, 1, looked()),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn flip_down_revokes_object_evidence_across_every_join() {
    let revealed = |region: Located<Node>| {
        body(
            1,
            vec![
                look(),
                reveal(2, looked()),
                region,
                public_choice(9, 1, looked()),
            ],
        )
    };
    accept(revealed(control(Control::Sequence {
        body: body(2, vec![]),
    })));
    for region in [
        flip(3, looked()),
        control(Control::Sequence {
            body: body(2, vec![flip(3, looked())]),
        }),
        control(Control::If {
            condition: Expr::Literal(Bool(true)),
            then_body: body(2, vec![flip(3, looked())]),
            else_body: None,
        }),
        simultaneous(vec![flip(3, looked())], 4),
        control(Control::Repeat {
            body: RepeatBody::Program {
                body: body(2, vec![flip(3, looked())]),
            },
            termination: Termination::Count {
                count: Expr::Literal(Nat(1)),
            },
            check: IterationCheck::BeforeIteration,
            scope: ScopeId(5),
            receipts: vec![],
        }),
    ] {
        reject(revealed(region), WellFormedReason::UnsupportedShape);
    }
    let chosen = Selector::Bound {
        reference: Binding::new(2, ScopeId(1)),
    };
    let field = zone(PlayerRef::Controller {}, ZoneKind::Field, 20);
    accept(body(
        1,
        vec![
            public_choice(2, 1, field.clone()),
            public_choice(6, 1, chosen.clone()),
        ],
    ));
    reject(
        body(
            1,
            vec![
                public_choice(2, 1, field),
                guarded_choice(5, 3),
                guarded(flip(3, chosen.clone()), 3),
                public_choice(6, 1, chosen),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn a_flip_later_in_a_loop_body_precedes_the_next_iteration() {
    let looped = |node: Located<Node>| body(1, vec![look(), reveal(2, looked()), node]);
    let repeat = |steps| {
        control(Control::Repeat {
            body: RepeatBody::Program {
                body: body(2, steps),
            },
            termination: Termination::Count {
                count: Expr::Literal(Nat(2)),
            },
            check: IterationCheck::BeforeIteration,
            scope: ScopeId(5),
            receipts: vec![],
        })
    };
    let each_object = |steps| {
        control(Control::EachObject {
            input: source_holder(),
            binding: Binding::new(1, ScopeId(2)),
            body: body(2, steps),
            order: None,
        })
    };
    let each_player = |steps| {
        control(Control::EachPlayer {
            players: controller_set(),
            player: Binding::new(1, ScopeId(3)),
            stages: vec![PlayerStage {
                body: body(3, steps),
                mode: StageMode::ChoiceProcedure {},
            }],
            choice_order: PlayerOrder::TurnPlayerFirst,
        })
    };
    accept(looped(repeat(vec![public_choice(9, 2, looked())])));
    accept(looped(each_object(vec![public_choice(9, 2, looked())])));
    accept(looped(each_player(vec![public_choice(9, 3, looked())])));
    reject(
        looped(repeat(vec![
            public_choice(9, 2, looked()),
            flip(3, looked()),
        ])),
        WellFormedReason::UnsupportedShape,
    );
    reject(
        looped(each_object(vec![
            public_choice(9, 2, looked()),
            flip(3, looked()),
        ])),
        WellFormedReason::UnsupportedShape,
    );
    reject(
        looped(each_player(vec![
            public_choice(9, 3, looked()),
            flip(3, looked()),
        ])),
        WellFormedReason::UnsupportedShape,
    );
}

fn nested_flip(flipped: bool) -> Vec<Located<Node>> {
    if flipped {
        vec![flip(3, looked())]
    } else {
        vec![]
    }
}
fn branching_containers(flipped: bool) -> Vec<Located<Node>> {
    let may = Decision {
        domain: ChoiceDomain::Compound { domains: vec![] },
        kind: DecisionKind::ResolutionChoice {
            may: Declinable::MayDecline,
            cardinality: bounds(1, Some(1)),
            precondition: None,
        },
        ..choose(30, 20)
    };
    vec![
        control(Control::If {
            condition: Expr::Literal(Bool(true)),
            then_body: body(20, nested_flip(flipped)),
            else_body: None,
        }),
        control(Control::If {
            condition: Expr::Literal(Bool(true)),
            then_body: body(20, vec![]),
            else_body: Some(body(21, nested_flip(flipped))),
        }),
        control(Control::Sequence {
            body: body(20, nested_flip(flipped)),
        }),
        control(Control::May {
            choice: may,
            body: body(20, nested_flip(flipped)),
            receipt: ReceiptId(31),
        }),
        control(Control::Pay {
            at: PaymentTime::ResolutionChoice,
            costs: CostSpec::Alternative {
                alternatives: vec![
                    CostSpec::Ordered { steps: vec![] },
                    CostSpec::Ordered { steps: vec![] },
                ],
            },
            optional: PaymentOptionality::Required,
            on_paid: body(20, nested_flip(flipped)),
            receipt: ReceiptId(31),
        }),
        if flipped {
            simultaneous(vec![flip(3, looked())], 31)
        } else {
            control(Control::Sequence {
                body: body(20, vec![]),
            })
        },
    ]
}
fn looping_containers(flipped: bool) -> Vec<Located<Node>> {
    vec![
        control(Control::Repeat {
            body: RepeatBody::Program {
                body: body(20, nested_flip(flipped)),
            },
            termination: Termination::Count {
                count: Expr::Literal(Nat(1)),
            },
            check: IterationCheck::BeforeIteration,
            scope: ScopeId(21),
            receipts: vec![],
        }),
        control(Control::EachObject {
            input: source_holder(),
            binding: Binding::new(1, ScopeId(20)),
            body: body(20, nested_flip(flipped)),
            order: None,
        }),
        control(Control::EachPlayer {
            players: controller_set(),
            player: Binding::new(1, ScopeId(20)),
            stages: vec![PlayerStage {
                body: body(20, nested_flip(flipped)),
                mode: StageMode::NonChoiceSimultaneous {},
            }],
            choice_order: PlayerOrder::TurnPlayerFirst,
        }),
        control(Control::Mode {
            modes: vec![Mode {
                id: ModeId(1),
                parameters: vec![],
                body: body(20, nested_flip(flipped)),
            }],
            count: Expr::Literal(Nat(1)),
            chosen: ModeDecision {
                id: DecisionId(32),
                actor: PlayerRef::Controller {},
                domain: ChoiceDomain::Modes {
                    modes: vec![ModeId(1)],
                    cardinality: bounds(1, Some(1)),
                    binding: Binding::new(33, ScopeId(10)),
                    exclude_used: None,
                },
                kind: ModeDecisionKind::ResolutionChoice {
                    cardinality: bounds(1, Some(1)),
                },
                visibility: Visibility::Public {},
            },
            choice_kind: ModeChoiceKind::ResolutionAlternative,
        }),
    ]
}

#[test]
fn a_flip_nested_anywhere_in_a_loop_body_closes_object_proof_from_the_loop_start() {
    let looped = |container: Located<Node>| {
        body(
            1,
            vec![
                look(),
                reveal(2, looked()),
                control(Control::Repeat {
                    body: RepeatBody::Program {
                        body: body(10, vec![public_choice(9, 10, looked()), container]),
                    },
                    termination: Termination::Count {
                        count: Expr::Literal(Nat(2)),
                    },
                    check: IterationCheck::BeforeIteration,
                    scope: ScopeId(11),
                    receipts: vec![],
                }),
            ],
        )
    };
    let quiet_containers = branching_containers(false)
        .into_iter()
        .chain(looping_containers(false));
    let flipping_containers = branching_containers(true)
        .into_iter()
        .chain(looping_containers(true));
    for (quiet, flipping) in quiet_containers.zip(flipping_containers) {
        accept(looped(quiet));
        reject(looped(flipping), WellFormedReason::UnsupportedShape);
    }
}

#[test]
fn after_a_possible_flip_no_new_observation_restores_object_proof() {
    let flipped = |steps: Vec<Located<Node>>| {
        let mut all = vec![flip(1, source_holder())];
        all.extend(steps);
        body(1, all)
    };
    let mut private_look = chosen_objects(4, 1, looked_at(2));
    private_look.visibility = private(PlayerRef::Controller {});
    let token = instruction(
        Action::Token {
            tokens: vec![TokenSpec {
                template: TokenTemplateId("synthetic-fairy".into()),
                name: Expr::Literal(CardName("フェアリー".into())),
                count: Expr::Literal(Nat(1)),
            }],
            destination: TokenDestination::Single {
                zone: Expr::Literal(ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Field,
                }),
            },
            output: Binding::new(3, ScopeId(1)),
            initial_state: None,
            initial_posture: Posture::Standing,
            entry_effects: vec![],
        },
        3,
        "SD01-006",
    );
    let field = zone(PlayerRef::Controller {}, ZoneKind::Field, 20);
    accept(flipped(vec![public_choice(5, 1, field.clone())]));
    for steps in [
        vec![look_into(2), decision(private_look)],
        vec![
            look_into(2),
            reveal(3, looked_at(2)),
            public_choice(4, 1, looked_at(2)),
        ],
        vec![
            token,
            public_choice(
                4,
                1,
                Selector::Created {
                    receipt: ReceiptReference::new(ReceiptId(3)),
                },
            ),
        ],
        vec![
            public_choice(5, 1, field),
            public_choice(
                6,
                1,
                Selector::Bound {
                    reference: Binding::new(5, ScopeId(1)),
                },
            ),
        ],
    ] {
        reject(flipped(steps), WellFormedReason::UnsupportedShape);
    }
}

#[test]
fn loop_elements_and_collected_receipts_keep_their_viewer() {
    let element = Binding::new(1, ScopeId(2));
    let each = |visibility| {
        let mut choice = chosen_objects(
            4,
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::Bound { binding: element }],
            },
        );
        choice.visibility = visibility;
        control(Control::EachObject {
            input: looked(),
            binding: element,
            body: body(2, vec![decision(choice)]),
            order: None,
        })
    };
    accept(body(
        1,
        vec![look(), each(private(PlayerRef::Controller {}))],
    ));
    reject(
        body(1, vec![look(), each(Visibility::Public {})]),
        WellFormedReason::UnsupportedShape,
    );
    let collected = Binding::new(6, ScopeId(1));
    let gather = control(Control::Repeat {
        body: RepeatBody::Program {
            body: body(2, vec![look()]),
        },
        termination: Termination::Count {
            count: Expr::Literal(Nat(2)),
        },
        check: IterationCheck::BeforeIteration,
        scope: ScopeId(5),
        receipts: vec![ReceiptExport::Look {
            source: ReceiptReference::new(ReceiptId(1)),
            iteration: Binding::new(7, ScopeId(5)),
            collect: Some(collected),
        }],
    });
    let from_all = |visibility| {
        let mut choice = chosen_objects(
            8,
            1,
            Selector::CollectedMembers {
                receipts: ObjectReceiptList::Look {
                    receipts: collected,
                },
                projection: MemberProjection::After,
                deduplicate: GenerationDeduplication::ByObjectAndGeneration,
            },
        );
        choice.visibility = visibility;
        decision(choice)
    };
    accept(body(
        1,
        vec![gather.clone(), from_all(private(PlayerRef::Controller {}))],
    ));
    reject(
        body(1, vec![gather, from_all(Visibility::Public {})]),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn a_play_capture_of_a_hand_stays_with_its_holder() {
    let captured = Binding::new(1, ScopeId(1));
    let make = |visibility| {
        let mut choice = chosen_objects(
            2,
            1,
            Selector::Captured {
                reference: CaptureRef { binding: captured },
            },
        );
        choice.visibility = visibility;
        let mut program = card(body(1, vec![decision(choice)]), "synthetic").node;
        program.abilities[0].node.play = Some(PlaySpec {
            limit: None,
            captures: vec![Capture::Objects {
                at: CaptureTime::AtPlay {
                    execution: ExecutionRef::Current {},
                },
                value: hand(),
                into: CaptureRef { binding: captured },
            }],
            parameters: vec![],
            costs: CostSpec::Ordered { steps: vec![] },
        });
        program
    };
    WellFormedCardProgram::check(make(private(PlayerRef::Controller {}))).unwrap();
    assert_eq!(
        WellFormedCardProgram::check(make(Visibility::Public {}))
            .unwrap_err()
            .reason,
        WellFormedReason::UnsupportedShape
    );
    // Revealing a zone shows its current members only, never a later re-evaluation.
    reject(
        body(
            1,
            vec![
                reveal(1, hand()),
                public_choice(2, 1, zone(PlayerRef::Controller {}, ZoneKind::Hand, 12)),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn flip_down_keeps_immutable_public_scalar_answers() {
    let answer = Binding::new(2, ScopeId(1));
    accept(body(
        1,
        vec![
            decision(choose(2, 1)),
            flip(3, source_holder()),
            discard(DiscardMode::Random {
                population: hand(),
                count: signed(Expr::Bound(answer)),
            }),
        ],
    ));
}

#[test]
fn a_play_capture_cannot_regain_evidence_revoked_before_a_mode_body() {
    let captured = Binding::new(1, ScopeId(1));
    let selection = Selector::Captured {
        reference: CaptureRef { binding: captured },
    };
    let make = |prefix: Vec<Located<Node>>| {
        let mode = control(Control::Mode {
            modes: vec![Mode {
                id: ModeId(1),
                parameters: vec![],
                body: body(2, vec![public_choice(9, 2, selection.clone())]),
            }],
            count: Expr::Literal(Nat(1)),
            chosen: ModeDecision {
                id: DecisionId(7),
                actor: PlayerRef::Controller {},
                domain: ChoiceDomain::Modes {
                    modes: vec![ModeId(1)],
                    cardinality: bounds(1, Some(1)),
                    binding: Binding::new(8, ScopeId(1)),
                    exclude_used: None,
                },
                kind: ModeDecisionKind::PlayParameter {
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                },
                visibility: Visibility::Public {},
            },
            choice_kind: ModeChoiceKind::ChoiceKeyword,
        });
        let mut steps = prefix;
        steps.push(mode);
        let mut program = card(body(1, steps), "synthetic").node;
        program.abilities[0].node.play = Some(PlaySpec {
            limit: None,
            captures: vec![Capture::Objects {
                at: CaptureTime::AtPlay {
                    execution: ExecutionRef::Current {},
                },
                value: zone(PlayerRef::Controller {}, ZoneKind::Field, 30),
                into: CaptureRef { binding: captured },
            }],
            parameters: vec![],
            costs: CostSpec::Ordered { steps: vec![] },
        });
        program
    };
    WellFormedCardProgram::check(make(vec![])).unwrap();
    assert_eq!(
        WellFormedCardProgram::check(make(vec![flip(3, selection.clone())]))
            .unwrap_err()
            .reason,
        WellFormedReason::UnsupportedShape
    );
}

#[test]
fn guards_in_a_batch_are_exact_and_only_the_protected_member_gets_the_answer() {
    let guard = Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        predicate: Expr::Literal(Bool(false)),
        protects: InstructionId(1),
        reason: PreconditionReason::CardText {},
    };
    let mut choice = choose(1, 1);
    if let DecisionKind::ResolutionChoice { precondition, .. } = &mut choice.kind {
        *precondition = Some(guard.clone());
    }
    let mut member = draw(1, signed(Expr::Bound(Binding::new(1, ScopeId(1)))));
    if let Node::Instruction { instruction } = &mut member.node {
        instruction.precondition = Some(guard);
    }
    let mut batch_node = simultaneous(vec![member], 10);
    if let Node::Control {
        control: Control::Simultaneous { batch, .. },
    } = &mut batch_node.node
    {
        batch.choice_stages.push(BatchChoiceStage {
            decisions: vec![choice],
            order: PlayerOrder::TurnPlayerFirst,
        });
    }
    accept(body(1, vec![batch_node.clone()]));
    let mut leaked = batch_node.clone();
    if let Node::Control {
        control: Control::Simultaneous { batch, .. },
    } = &mut leaked.node
    {
        let Node::Instruction { instruction } =
            draw(2, signed(Expr::Bound(Binding::new(1, ScopeId(1))))).node
        else {
            panic!("instruction")
        };
        batch.members.push(Located {
            node: *instruction,
            source: source("synthetic"),
        });
    }
    reject(body(1, vec![leaked]), WellFormedReason::UnavailableBinding);
    if let Node::Control {
        control: Control::Simultaneous { batch, .. },
    } = &mut batch_node.node
    {
        batch.members[0]
            .node
            .precondition
            .as_mut()
            .unwrap()
            .predicate = positive(Expr::Bound(Binding::new(1, ScopeId(1))));
    }
    reject(
        body(1, vec![batch_node]),
        WellFormedReason::UnavailableBinding,
    );
}
