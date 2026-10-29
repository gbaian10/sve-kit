use super::*;
use core::slice::from_mut;
use sve_engine::game::internal::{DamageShare, DecisionView, InputDomain};

pub(super) fn spell(steps: Vec<Located<Node>>) -> Located<Ability> {
    let mut ability = payment::paid_ability(vec![payment::pp(1)], steps);
    ability.source = source("unit-spell");
    ability.source.fragment = None;
    ability.node.kind = AbilityKind::Spell;
    ability.node.active_zones = vec![ZoneKind::Resolution];
    locate_steps(
        &mut ability.node.body.as_mut().unwrap().steps,
        &ability.source,
        &ability.node.source,
    );
    ability
}

pub(super) fn position(program: &WellFormedCardProgram, zone: &str, followers: usize) -> Game {
    let mut initial = setup(1, json!([{"id":"d","card":"unit-follower"}]));
    initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"s","card":"unit-spell"}]);
    initial["players"]["P2"]["zones"]["field"] = json!(
        (0..followers)
            .map(|i| json!({"id":format!("f{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "formal-play",
    )
    .unwrap();
    game.load_internal_card("unit-spell", program).unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

#[expect(
    clippy::panic,
    reason = "An unexpected fixture result must fail the test."
)]
fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(view) = play else {
        panic!("expected formal question");
    };
    view
}

const fn parameter(id: u64, slot: PlaySlot, domain: ChoiceDomain) -> Decision {
    Decision {
        id: DecisionId(id),
        captures: vec![],
        actor: PlayerRef::Controller {},
        domain,
        kind: DecisionKind::PlayParameter {
            slot,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        visibility: Visibility::Public {},
    }
}

pub(super) fn target(id: u64, scope: u64, minimum: u64, maximum: u64) -> Decision {
    let cardinality = Cardinality {
        minimum: Nat(minimum),
        maximum: Some(Nat(maximum)),
    };
    parameter(
        id,
        PlaySlot::Targets,
        ChoiceDomain::Objects {
            constraint: SelectionConstraint {
                groups: vec![SelectionGroup {
                    selector: zone(ZoneKind::Field, 90_u64.checked_add(id).unwrap()),
                    cardinality,
                }],
                cardinality,
                distinct_by: None,
                disjoint: Disjointness::Disjoint,
                aggregate_bounds: vec![],
            },
            binding: Binding::new(id, ScopeId(scope)),
            target_roles: None,
        },
    )
}

const fn selected(id: u64, scope: u64) -> RecipientSet {
    RecipientSet::Objects {
        objects: Selector::Bound {
            reference: Binding::new(id, ScopeId(scope)),
        },
    }
}

fn capture_pp(decision: &mut Decision, slot: u64, scope: u64) -> CaptureRef<Nat> {
    let into = CaptureRef {
        binding: Binding::new(slot, ScopeId(scope)),
    };
    decision.captures.push(Capture::Natural {
        at: CaptureTime::AtSelection {
            decision: decision.id,
        },
        value: Expr::Compute(Box::new(NatOperation::CheckedInt {
            value: Expr::Live(Box::new(IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::PpCurrent,
            })),
        })),
        into,
    });
    into
}

#[test]
fn ordinary_spell_uses_resolution_generation_and_single_payment_before_check_timing() {
    let program = card(vec![spell(vec![draw(1, 1)])]);
    for origin in ["hand", "ex"] {
        let mut game = position(&program, origin, 5);
        let result = resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        assert_eq!(result.source.holder.generation, Generation(1));
        assert_eq!(
            result.progress.receipts[&ReceiptId(100)],
            InstructionOutcome::Payment {
                quantity: Quantity {
                    amount: Int(1),
                    unit: QuantityUnit::Pp
                },
                subjects: vec![]
            }
        );
        assert!(
            matches!(result.progress.facts.as_slice(), [Fact::CardPlayed { object, execution, .. }, Fact::Draw(_), Fact::CardResolved { object: resolved, execution: finished, .. }] if object == resolved && execution == finished)
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 3);
        assert_eq!(view["P1"]["hand"][0], "d");
        assert_eq!(view["P1"]["cemetery"], json!(["s"]));
        assert_eq!(game.internal_card_stage(), Some(ResumeStage::CheckTiming));
        game.play_internal_card("s", PlayerId { seat: 1 })
            .unwrap_err();
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["phase"],
            "main"
        );
    }
    let mut wrong = spell(vec![draw(1, 1)]);
    wrong.node.active_zones = vec![ZoneKind::Hand, ZoneKind::Ex];
    let mut game = game(1, 1);
    let before = exact_authority(&game);
    game.load_internal_card("unit-spell", &card(vec![wrong]))
        .unwrap_err();
    assert_eq!(before, exact_authority(&game));
}

#[expect(
    clippy::panic,
    reason = "The fixture must retain its Damage instruction."
)]
fn distribution_spell(minimum_targets: u64, predicate: Expr<Bool>) -> WellFormedCardProgram {
    let targets = selected(10, 1);
    let mut allocated = damage(1, targets.clone(), 0);
    let Node::Instruction { instruction } = &mut allocated.node else {
        panic!("damage");
    };
    let Action::Damage { amount, .. } = &mut instruction.action else {
        panic!("damage");
    };
    *amount = DamageAmount::Allocation {
        binding: Binding::new(11, ScopeId(1)),
    };
    let mut a = spell(vec![
        allocated,
        damage(
            2,
            RecipientSet::Leaders {
                players: players(PlayerRelation::Opponents),
            },
            1,
        ),
        draw_value(
            3,
            Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Captured(CaptureRef {
                    binding: Binding::new(80, ScopeId(1)),
                }),
            })),
        ),
    ]);
    let play = a.node.play.as_mut().unwrap();
    play.captures.push(Capture::Natural {
        at: CaptureTime::AtPlay {
            execution: ExecutionRef::Current {},
        },
        value: Expr::Live(Box::new(NatRead::ZoneCount {
            selector: zone(ZoneKind::Ex, 91),
        })),
        into: CaptureRef {
            binding: Binding::new(80, ScopeId(1)),
        },
    });
    let mut target = target(10, 1, minimum_targets, 5);
    let ChoiceDomain::Objects { constraint, .. } = &mut target.domain else {
        panic!("targets");
    };
    let Selector::Zone { predicate: p, .. } = &mut constraint.groups[0].selector else {
        panic!("field targets");
    };
    **p = predicate;
    let pp = capture_pp(&mut target, 81, 1);
    play.parameters = vec![
        target,
        parameter(
            11,
            PlaySlot::Distribution,
            ChoiceDomain::Distribution {
                recipients: targets,
                total: Expr::Captured(pp),
                minimum: Nat(1),
                binding: Binding::new(11, ScopeId(1)),
            },
        ),
    ];
    card(vec![a])
}

#[test]
fn bp08_028_distribution_seed_retries_targets_and_freezes_pre_payment_values() {
    let mut game = position(&distribution_spell(0, Expr::Literal(Bool(true))), "ex", 5);
    let first = awaiting(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    assert!(matches!(first.domain, InputDomain::Targets(_)));
    let retry = awaiting(
        game.answer_internal_card(
            first.actor,
            first.token,
            Answer::Targets(vec![0, 1, 2, 3, 4]),
        )
        .unwrap(),
    );
    assert!(retry.token.epoch > first.token.epoch);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4
    );
    let before = exact_authority(&game);
    game.answer_internal_card(first.actor, first.token, Answer::Targets(vec![0, 1]))
        .unwrap_err();
    assert_eq!(before, exact_authority(&game));
    let distribution = awaiting(
        game.answer_internal_card(retry.actor, retry.token, Answer::Targets(vec![0, 1]))
            .unwrap(),
    );
    assert!(
        matches!(&distribution.domain, InputDomain::Distribution(domain) if domain.total == Nat(4) && domain.minimum == Nat(1))
    );
    let before = exact_authority(&game);
    game.answer_internal_card(
        distribution.actor,
        distribution.token,
        Answer::Distribution(vec![
            DamageShare {
                handle: 0,
                amount: Nat(4),
            },
            DamageShare {
                handle: 1,
                amount: Nat(0),
            },
        ]),
    )
    .unwrap_err();
    assert_eq!(before, exact_authority(&game));
    let result = resolved(
        game.answer_internal_card(
            distribution.actor,
            distribution.token,
            Answer::Distribution(vec![
                DamageShare {
                    handle: 0,
                    amount: Nat(3),
                },
                DamageShare {
                    handle: 1,
                    amount: Nat(1),
                },
            ]),
        )
        .unwrap(),
    );
    assert_eq!(
        result.progress.receipts[&ReceiptId(1)],
        InstructionOutcome::Damage(DamageQuantityOutcome {
            requested: Int(4),
            actual: Nat(4)
        })
    );
    cards(&result.progress, 3, 1, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3
    );
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f1"]["hp"],
        2
    );
    let mut empty = position(&distribution_spell(0, Expr::Literal(Bool(true))), "ex", 0);
    let targets = awaiting(empty.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let result = resolved(
        empty
            .answer_internal_card(targets.actor, targets.token, Answer::Targets(vec![]))
            .unwrap(),
    );
    assert_eq!(
        result.progress.receipts[&ReceiptId(1)],
        InstructionOutcome::Damage(DamageQuantityOutcome {
            requested: Int(0),
            actual: Nat(0)
        })
    );
    assert_eq!(
        empty.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        19
    );
    cards(&result.progress, 3, 1, 1);
}

#[test]
fn impossible_or_live_predicate_targets_reject_before_any_target_question() {
    let mut impossible = position(&distribution_spell(5, Expr::Literal(Bool(true))), "ex", 5);
    let before = exact_authority(&impossible);
    let error = impossible
        .play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(matches!(error, sve_engine::EngineFailure::IllegalPlay(_)));
    assert_eq!(before, exact_authority(&impossible));
    assert_eq!(impossible.internal_decision(View::Referee), None);
    // Payment changes PP, while resolution rechecks only target generations.
    let paid_from_four = Expr::Compute(Box::new(Predicate::CompareInt {
        left: Expr::Live(Box::new(IntRead::PlayerResource {
            player: PlayerRef::Controller {},
            property: PlayerResource::PpCurrent,
        })),
        comparison: Comparison::GreaterEqual,
        right: Expr::Literal(Int(4)),
    }));
    let mut empty = game(1, 1);
    let before = exact_authority(&empty);
    empty
        .load_internal_card("unit-spell", &distribution_spell(0, paid_from_four))
        .unwrap_err();
    assert_eq!(before, exact_authority(&empty));
}

#[test]
fn choice_keyword_precedes_targets_and_only_selected_modes_need_current_targets() {
    let chosen = ModeDecision {
        id: DecisionId(20),
        actor: PlayerRef::Controller {},
        domain: ChoiceDomain::Modes {
            modes: vec![ModeId(1), ModeId(2)],
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: None,
            },
            binding: Binding::new(20, ScopeId(1)),
            exclude_used: None,
        },
        kind: ModeDecisionKind::PlayParameter {
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        visibility: Visibility::Public {},
    };
    let mode = node(Node::Control {
        control: Control::Mode {
            modes: vec![
                Mode {
                    id: ModeId(1),
                    parameters: vec![target(10, 2, 1, 1)],
                    body: body(2, vec![damage(1, selected(10, 2), 2)]),
                },
                Mode {
                    id: ModeId(2),
                    parameters: vec![],
                    body: body(3, vec![draw(2, 1)]),
                },
            ],
            count: Expr::Compute(Box::new(NatOperation::Conditional {
                condition: Expr::Compute(Box::new(Predicate::And {
                    predicates: vec![
                        Expr::Compute(Box::new(Predicate::CompareNat {
                            left: Expr::Frozen(Box::new(NatFrozen::SpellChain {
                                player: PlayerRef::Controller {},
                                execution: ExecutionRef::Current {},
                                at: SpellCountTime::AtPlay,
                            })),
                            comparison: Comparison::Equal,
                            right: Expr::Literal(Nat(0)),
                        })),
                        Expr::Compute(Box::new(Predicate::CompareNat {
                            left: Expr::Frozen(Box::new(NatFrozen::NecroCharge {
                                player: PlayerRef::Controller {},
                                execution: ExecutionRef::Current {},
                                at: SpellCountTime::AtPlay,
                            })),
                            comparison: Comparison::Equal,
                            right: Expr::Literal(Nat(0)),
                        })),
                    ],
                })),
                then_value: Expr::Literal(Nat(1)),
                else_value: Expr::Literal(Nat(2)),
            })),
            chosen,
            choice_kind: ModeChoiceKind::ChoiceKeyword,
        },
    });
    let mut a = spell(vec![mode]);
    locate_steps(
        &mut a.node.body.as_mut().unwrap().steps,
        &a.source,
        &a.node.source,
    );
    let program = card(vec![a]);
    let mut game = position(&program, "hand", 0);
    let modes = awaiting(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    assert!(matches!(
        modes.domain,
        InputDomain::Modes { count: Nat(1), .. }
    ));
    let mut absent = game.clone();
    assert!(matches!(
        absent.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(1)])),
        Err(sve_engine::EngineFailure::Invalid(_))
    ));
    transactions::assert_restored(&absent, &game);
    let result = resolved(
        game.answer_internal_card(modes.actor, modes.token, Answer::Modes(vec![ModeId(2)]))
            .unwrap(),
    );
    cards(&result.progress, 2, 1, 1);
}

#[test]
fn base_activated_play_restores_payment_and_body_observations_after_a_valid_answer_fails() {
    let mut target = target(10, 1, 1, 1);
    let pp = capture_pp(&mut target, 81, 1);
    let mut a = payment::paid_ability(
        vec![payment::pp(1), payment::act()],
        vec![
            draw_value(
                1,
                Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Captured(pp),
                })),
            ),
            draw(2, 10_001),
        ],
    );
    a.source.face = "base".into();
    a.node.source.origin = AbilityOrigin::Printed {
        face: FaceId("base".into()),
        line: Nat(1),
    };
    locate_steps(
        &mut a.node.body.as_mut().unwrap().steps,
        &a.source,
        &a.node.source,
    );
    a.node.play.as_mut().unwrap().parameters = vec![target];
    if let CostSpec::Ordered { steps } = &mut a.node.play.as_mut().unwrap().costs {
        for step in steps {
            step.instruction.source = a.node.source.clone();
        }
    }
    let mut game = position_with_card(1, &card(vec![a]));
    let origin = game.clone();
    let question = awaiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let before = exact_authority(&game);
    game.answer_internal_card(
        PlayerId { seat: 2 },
        question.token,
        Answer::Targets(vec![0]),
    )
    .unwrap_err();
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0, 0]))
        .unwrap_err();
    assert_eq!(before, exact_authority(&game));
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
        .unwrap_err();
    transactions::assert_restored(&game, &origin);
    assert_eq!(game.internal_card_stage(), None);
    let next = awaiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(next.token.epoch > question.token.epoch);
}

fn discounted_spell() -> WellFormedCardProgram {
    let mut spell = spell(vec![draw(4, 1)]);
    spell.node.play.as_mut().unwrap().costs = payment::paid_ability(vec![payment::pp(5)], vec![])
        .node
        .play
        .unwrap()
        .costs;
    let mut damage = damage(
        5,
        RecipientSet::Leaders {
            players: players(PlayerRelation::Opponents),
        },
        0,
    );
    if let Node::Instruction { instruction } = &mut damage.node
        && let Action::Damage { amount, .. } = &mut instruction.action
    {
        *amount = DamageAmount::Uniform {
            amount: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                information: NumberRead::Cost {
                    object: ObjectView::SourceInformation {},
                    view: CharacteristicView::Effective,
                },
            })),
        };
    }
    locate_steps(from_mut(&mut damage), &spell.source, &spell.node.source);
    spell.node.body.as_mut().unwrap().steps.push(damage);
    let condition = Expr::Compute(Box::new(Predicate::Exists {
        objects: zone(ZoneKind::Field, 80),
    }));
    let modifier = Located {
        source: spell.source.clone(),
        node: PlayCostModifier {
            condition: Some(condition),
            change: CostChange::Delta {
                amount: Expr::Literal(Int(-2)),
            },
        },
    };
    WellFormedCardProgram::check(CardProgram {
        abilities: vec![spell],
        card_play_costs: vec![Located {
            source: modifier.source.clone(),
            node: FacePlayCost {
                exchange: None,
                face: FaceId("0".into()),
                modifiers: vec![modifier],
            },
        }],
    })
    .unwrap()
}

#[test]
fn ex_spell_preserves_the_installed_reduction_until_its_single_intrinsic_payment() {
    let initial = setup(
        1,
        json!([{"id":"d","card":"unit-spell"}, {"id":"next","card":"unit-follower"}]),
    );
    let mut game = Game::new(
        catalog_with_cost(&json!("5")),
        &initial,
        &Value::Null,
        &Value::Null,
        "ex-spell",
    )
    .unwrap();
    let mut reduction = cost::reduce(
        3,
        MovementReceipt::Move {
            receipt: ReceiptReference::new(ReceiptId(2)),
        },
    );
    if let Node::Instruction { instruction } = &mut reduction.node
        && let Action::InstallEffect { effect, .. } = &mut instruction.action
        && let Effect::CostModifier { change, .. } = effect.as_mut()
    {
        *change = CostChange::Delta {
            amount: Expr::Literal(Int(-1)),
        };
    }
    let result = game
        .execute_internal(
            &checked(vec![
                ordering::look(1, 1),
                instruction(
                    2,
                    Action::Move {
                        objects: ordering::looked(1),
                        movement: MoveSpec {
                            to: MoveDestination::Shared {
                                zone: ZoneRef {
                                    player: PlayerRef::Controller {},
                                    kind: ZoneKind::Ex,
                                },
                            },
                            initial_posture: None,
                            entry_effects: vec![],
                            position: None,
                            order: None,
                        },
                        reason: MoveReason::Put,
                    },
                ),
                reduction,
            ]),
            &source("BP18-059"),
            context(1),
        )
        .unwrap();
    assert!(result.complete);
    game.load_internal_card("unit-spell", &discounted_spell())
        .unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    let result = resolved(game.play_internal_card("d", PlayerId { seat: 1 }).unwrap());
    assert_eq!(result.source.holder.generation, Generation(2));
    assert_eq!(
        result.progress.receipts[&ReceiptId(100)],
        InstructionOutcome::Payment {
            quantity: Quantity {
                amount: Int(2),
                unit: QuantityUnit::Pp
            },
            subjects: vec![]
        }
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        0
    );
    cards(&result.progress, 4, 1, 1);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        15
    );
}

#[test]
fn surrender_during_formal_target_selection_preserves_resolution_prefix() {
    let mut ability = spell(vec![damage(1, selected(10, 1), 2)]);
    ability.node.play.as_mut().unwrap().parameters = vec![target(10, 1, 1, 1)];
    let mut game = position(&card(vec![ability]), "hand", 1);
    let question = awaiting(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let before = game.projection(View::Referee).unwrap();
    let terminal = game.concede_internal(PlayerId { seat: 2 }).unwrap();
    let progress = terminal.progress.unwrap();
    assert!(!progress.complete);
    assert!(!progress.facts.iter().any(|fact| matches!(
        fact,
        Fact::CardPlayed { .. } | Fact::CardResolved { .. } | Fact::Damage(_)
    )));
    let view = game.projection(View::Referee).unwrap();
    for seat in ["P1", "P2"] {
        assert_eq!(before[seat], view[seat]);
    }
    assert_eq!(terminal.step.outcome, "game-end");
    assert!(game.internal_decision(View::Referee).is_none());
    assert_eq!(game.internal_card_stage(), None);
    let ended = exact_authority(&game);
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
        .unwrap_err();
    assert_eq!(ended, exact_authority(&game));
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture shapes must fail the test."
)]
fn trait_target_ability() -> Located<Ability> {
    let candidate = ObjectView::Current {
        object: Box::new(ObjectOperand::Bound {
            binding: Binding::new(9, ScopeId(100)),
        }),
    };
    let predicate = Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            Expr::Compute(Box::new(Predicate::KindEquals {
                left: Expr::Live(Box::new(KindRead::Kind {
                    object: candidate.clone(),
                    view: CharacteristicView::Effective,
                })),
                right: Expr::Literal(CardKind::Follower),
            })),
            Expr::Compute(Box::new(Predicate::HasTrait {
                traits: Expr::Live(Box::new(TraitsRead::Traits {
                    object: candidate,
                    view: CharacteristicView::Effective,
                })),
                trait_id: TraitId("兵士".into()),
            })),
        ],
    }));
    let mut target = target(10, 1, 1, 1);
    let ChoiceDomain::Objects { constraint, .. } = &mut target.domain else {
        panic!("target");
    };
    let Selector::Zone {
        predicate: filter, ..
    } = &mut constraint.groups[0].selector
    else {
        panic!("field");
    };
    **filter = predicate;
    let subjects = Selector::Bound {
        reference: Binding::new(10, ScopeId(1)),
    };
    let Node::Instruction {
        instruction: payment,
    } = movement::put(
        100,
        Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        },
        OwnerZone::Cemetery,
    )
    .node
    else {
        panic!("payment");
    };
    let mut a = payment::paid_ability(
        vec![payment::pp(1), payment.action],
        vec![
            instruction(
                1,
                Action::Damage {
                    recipients: selected(10, 1),
                    amount: DamageAmount::Uniform {
                        amount: Expr::Literal(Int(2)),
                    },
                    flags: DamageFlags::Ability,
                    source: ObjectView::SourceInformation {},
                    batch: BatchPolicy::Instruction {},
                },
            ),
            numeric::modify(
                2,
                subjects.clone(),
                numeric::pair(1, 1),
                DurationEnd::NoTemporalExpiry {},
            ),
            instruction(3, Action::Destroy { objects: subjects }),
            draw(4, 1),
        ],
    );
    a.node.play.as_mut().unwrap().parameters = vec![target];
    a
}

fn trait_target_position() -> Game {
    let snapshot = ["unit-follower", "other"].iter().map(|number| json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":"フォロワー","cost":"1","power":"2","hp":"3","traits":if *number == "unit-follower" { json!(["兵士"]) } else { json!([]) },"text":null,"sections":[]}]}).to_string()).collect::<Vec<_>>().join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string(),
            &[(
                "trait-target.yaml".into(),
                json!({"format":1,"kind":"effect_set","cards":{
                    "unit-follower":{"status":"complete","review":"synthetic","abilities":[]},
                    "other":{"status":"complete","review":"synthetic","abilities":[]}
                }})
                .to_string(),
            )],
        )
        .unwrap(),
    );
    let mut initial = setup(1, json!([{"id":"d","card":"other"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower"}, {"id":"nonmatch","card":"other"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([]);
    Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "trait-target",
    )
    .unwrap()
}

#[test]
fn stable_trait_target_retains_selection_after_paid_departure_and_runs_independent_draw() {
    let a = trait_target_ability();
    let mut game = trait_target_position();
    let origin = exact_authority(&game);
    let mut flipping = a.clone();
    flipping.node.body.as_mut().unwrap().steps.push(instruction(
        5,
        Action::Modify {
            subjects: zone(ZoneKind::Field, 990),
            patch: CharacteristicPatch::SetKind {
                kind: CardKind::Amulet,
            },
            expires: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                intervals: vec![],
                usage: None,
            },
            source: EffectReference::CurrentInstallation {},
        },
    ));
    locate_steps(
        &mut flipping.node.body.as_mut().unwrap().steps,
        &flipping.source,
        &flipping.node.source,
    );
    game.load_internal_card("unit-follower", &card(vec![flipping]))
        .unwrap_err();
    assert_eq!(origin, exact_authority(&game));
    game.load_internal_card("unit-follower", &card(vec![a]))
        .unwrap();
    game.load_internal_card("other", &card(vec![])).unwrap();
    let question = awaiting(
        game.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(
        matches!(&question.domain, InputDomain::Targets(domain) if domain.members.len() == 1 && domain.members[0].card.as_deref() == Some("unit-follower"))
    );
    let before = exact_authority(&game);
    game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![1]))
        .unwrap_err();
    assert_eq!(before, exact_authority(&game));
    let result = resolved(
        game.answer_internal_card(question.actor, question.token, Answer::Targets(vec![0]))
            .unwrap(),
    );
    assert_eq!(
        result.progress.receipts[&ReceiptId(1)],
        InstructionOutcome::Damage(DamageQuantityOutcome {
            requested: Int(2),
            actual: Nat(0)
        })
    );
    assert!(
        matches!(&result.progress.receipts[&ReceiptId(2)], InstructionOutcome::Modified { subjects, effect: None } if subjects.is_empty())
    );
    assert!(
        matches!(&result.progress.receipts[&ReceiptId(3)], InstructionOutcome::Destroy(outcome) if outcome.successful.is_empty())
    );
    cards(&result.progress, 4, 1, 1);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["pp"]["current"], 1);
    assert_eq!(view["P1"]["hand"], json!(["d"]));
    assert_eq!(view["P1"]["cemetery"], json!(["s"]));
    assert_eq!(view["objects"]["nonmatch"]["hp"], 3);
}

fn literal_choice(exact: bool, excluded: bool, targets: bool) -> Located<Node> {
    node(Node::Control {
        control: Control::Mode {
            modes: vec![
                Mode {
                    id: ModeId(1),
                    parameters: if targets {
                        vec![target(10, 2, 1, 1)]
                    } else {
                        vec![]
                    },
                    body: body(2, vec![draw(1, 0)]),
                },
                Mode {
                    id: ModeId(2),
                    parameters: vec![],
                    body: body(3, vec![draw(2, 0)]),
                },
            ],
            count: Expr::Literal(Nat(if excluded { 1 } else { 2 })),
            chosen: ModeDecision {
                id: DecisionId(20),
                actor: PlayerRef::Controller {},
                domain: ChoiceDomain::Modes {
                    modes: vec![ModeId(1), ModeId(2)],
                    cardinality: Cardinality {
                        minimum: Nat(if exact && !excluded { 2 } else { 1 }),
                        maximum: Some(Nat(if excluded { 1 } else { 2 })),
                    },
                    binding: Binding::new(20, ScopeId(1)),
                    exclude_used: excluded.then_some(ModeExclusion::CurrentTurnAbilityInstance),
                },
                kind: ModeDecisionKind::PlayParameter {
                    play_scope: ExecutionRef::Current {},
                    input: InputMode::Inline,
                },
                visibility: Visibility::Public {},
            },
            choice_kind: ModeChoiceKind::ChoiceKeyword,
        },
    })
}

#[test]
fn upto_modes_filter_targets_validate_answers_and_resolve_in_printed_order() {
    let mut mode = literal_choice(false, false, false);
    if let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut mode.node
    {
        modes[0].body.steps = vec![draw(1, 1)];
        modes[1].body.steps = vec![draw(2, 1)];
    }
    let program = card(vec![spell(vec![mode])]);
    let mut game = position(&program, "hand", 0);
    let question = awaiting(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let before = game.clone();
    assert!(matches!(
        game.answer_internal_card(question.actor, question.token, Answer::Modes(vec![])),
        Err(sve_engine::EngineFailure::Invalid(_))
    ));
    assert!(matches!(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(1), ModeId(1)])
        ),
        Err(sve_engine::EngineFailure::Invalid(_))
    ));
    transactions::assert_restored(&game, &before);
    let mut one = game.clone();
    let result = resolved(
        one.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(2)]),
        )
        .unwrap(),
    );
    assert!(result.progress.receipts.contains_key(&ReceiptId(2)));
    assert!(!result.progress.receipts.contains_key(&ReceiptId(1)));
    let result = resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Modes(vec![ModeId(2), ModeId(1)]),
        )
        .unwrap(),
    );
    cards(&result.progress, 1, 1, 1);
    cards(&result.progress, 2, 1, 0);
    let mut limited = position(
        &card(vec![spell(vec![literal_choice(false, false, true)])]),
        "hand",
        0,
    );
    let question = awaiting(
        limited
            .play_internal_card("s", PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(
        matches!(&question.domain, InputDomain::Modes { modes, .. } if *modes == vec![ModeId(2)])
    );
    resolved(
        limited
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Modes(vec![ModeId(2)]),
            )
            .unwrap(),
    );
    let mut exact = position(
        &card(vec![spell(vec![literal_choice(true, false, true)])]),
        "hand",
        0,
    );
    let before = exact.clone();
    assert!(matches!(
        exact.play_internal_card("s", PlayerId { seat: 1 }),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    transactions::assert_restored(&exact, &before);
    assert_unadmitted_modes();
}

fn assert_unadmitted_modes() {
    let mut arbitrary = literal_choice(false, false, false);
    if let Node::Control {
        control: Control::Mode { chosen, .. },
    } = &mut arbitrary.node
        && let ChoiceDomain::Modes { cardinality, .. } = &mut chosen.domain
    {
        cardinality.minimum = Nat(0);
    }
    let arbitrary = card(vec![spell(vec![arbitrary])]);
    let mut rejected = game(1, 1);
    assert!(
        matches!(
            rejected.load_internal_card("unit-spell", &arbitrary),
            Err(sve_engine::EngineFailure::Unsupported(_))
        ),
        "Any-number modes have no D1 admission"
    );
    let mut quick = ability(1, vec![literal_choice(true, true, false)]);
    quick.node.source.origin = AbilityOrigin::Keyword {
        keyword: KeywordId::Quick,
        origin: Box::new(quick.node.source.origin.clone()),
    };
    locate_steps(
        &mut quick.node.body.as_mut().unwrap().steps,
        &quick.source,
        &quick.node.source,
    );
    let mut allowed = quick.clone();
    if let Node::Control {
        control: Control::Mode { chosen, .. },
    } = &mut allowed.node.body.as_mut().unwrap().steps[0].node
        && let ChoiceDomain::Modes { exclude_used, .. } = &mut chosen.domain
    {
        *exclude_used = None;
    }
    rejected
        .clone()
        .load_internal_card("unit-follower", &card(vec![allowed]))
        .unwrap();
    assert!(
        matches!(
            rejected.load_internal_card("unit-follower", &card(vec![quick])),
            Err(sve_engine::EngineFailure::Unsupported(_))
        ),
        "Quick history exclusion needs a separate preflight proof"
    );
}

pub(super) fn next_turn(game: &mut Game) {
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    quick::pass(game);
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
}

#[expect(clippy::panic, reason = "The fixture must retain its mode group.")]
fn excluded_position() -> Game {
    let mut initial = setup(
        1,
        json!([{"id":"d1","card":"unit-spell"},{"id":"d2","card":"unit-spell"}]),
    );
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower"},{"id":"u","card":"unit-follower"}]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"o1","card":"unit-spell"},{"id":"o2","card":"unit-spell"}]);
    let mode = literal_choice(true, true, false);
    let mut a = payment::paid_ability(vec![payment::pp(1)], vec![mode]);
    let Node::Control {
        control: Control::Mode { modes, .. },
    } = &mut a.node.body.as_mut().unwrap().steps[0].node
    else {
        panic!("mode");
    };
    modes[0].body.steps = vec![node(Node::Control {
        control: Control::If {
            condition: Expr::Literal(Bool(false)),
            then_body: body(4, vec![draw(3, 1)]),
            else_body: None,
        },
    })];
    locate_steps(
        &mut a.node.body.as_mut().unwrap().steps,
        &a.source,
        &a.node.source,
    );
    let program = card(vec![a]);
    initial["players"]["P1"]["pp"] = json!({"current":0,"max":4});
    let mut failed = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "mode-history",
    )
    .unwrap();
    failed
        .load_internal_card("unit-follower", &program)
        .unwrap();
    failed
        .load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    failed
}

#[test]
fn excluded_modes_commit_only_on_play_and_are_scoped_to_instance_and_turn() {
    let mut failed = excluded_position();
    let before = failed.clone();
    assert!(matches!(
        failed.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 }),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    transactions::assert_restored(&failed, &before);
    next_turn(&mut failed);
    next_turn(&mut failed);
    let q = awaiting(
        failed
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let first = resolved(
        failed
            .answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(1)]))
            .unwrap(),
    );
    failed.resume_internal_card().unwrap();
    let q = awaiting(
        failed
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    assert!(matches!(&q.domain, InputDomain::Modes { modes, .. } if *modes == vec![ModeId(2)]));
    assert!(matches!(
        failed.answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(1)])),
        Err(sve_engine::EngineFailure::Invalid(_))
    ));
    resolved(
        failed
            .answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(2)]))
            .unwrap(),
    );
    failed.resume_internal_card().unwrap();
    assert!(matches!(
        failed.activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 }),
        Err(sve_engine::EngineFailure::IllegalPlay(_))
    ));
    let q = awaiting(
        failed
            .activate_internal_card("u", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    let other = resolved(
        failed
            .answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(1)]))
            .unwrap(),
    );
    assert_ne!(first.source.instance, other.source.instance);
    failed.resume_internal_card().unwrap();
    next_turn(&mut failed);
    next_turn(&mut failed);
    let q = awaiting(
        failed
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
    );
    resolved(
        failed
            .answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(1)]))
            .unwrap(),
    );
}

#[test]
fn distinct_name_play_rejects_an_impossible_selection_before_payment() {
    let mut ability = spell(vec![]);
    let mut decision = target(1, 1, 2, 2);
    let ChoiceDomain::Objects { constraint, .. } = &mut decision.domain else {
        panic!("object target");
    };
    constraint.distinct_by = Some(ObjectProperty::Names);
    ability
        .node
        .play
        .as_mut()
        .unwrap()
        .parameters
        .push(decision);
    let program = card(vec![ability]);
    let mut game = position(&program, "hand", 2);
    let before = format!("{game:?}");
    game.play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(
        format!("{game:?}") == before,
        "an impossible target group cannot consume payment"
    );
}

#[test]
fn resolving_spell_returns_only_itself_to_ex_or_cleans_up_when_ex_is_full() {
    let program = card(vec![spell(vec![
        draw(1, 1),
        movement::put(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Ex,
        ),
    ])]);
    for (origin, fillers, returned) in
        [("hand", 0_usize, true), ("hand", 5, false), ("ex", 4, true)]
    {
        let mut initial = setup(1, json!([{"id":"drawn","card":"unit-follower"}]));
        initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!(
            (0..fillers)
                .map(|n| json!({"id":format!("fill{n}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let source = json!({"id":"source","card":"unit-spell"});
        if origin == "ex" {
            initial["players"]["P1"]["zones"][origin]
                .as_array_mut()
                .unwrap()
                .push(source);
        } else {
            initial["players"]["P1"]["zones"][origin] = json!([source]);
        }
        let mut game =
            Game::new(catalog(), &initial, &Value::Null, &Value::Null, "self-ex").unwrap();
        game.load_internal_card("unit-spell", &program).unwrap();
        game.load_internal_card("unit-follower", &card(vec![]))
            .unwrap();
        resolved(
            game.play_internal_card("source", PlayerId { seat: 1 })
                .unwrap(),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 3);
        assert_eq!(state["P1"]["hand"], json!(["drawn"]));
        assert_eq!(state["P1"]["resolution"], json!([]));
        assert_eq!(
            state["P1"]["ex"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            returned
        );
        assert_eq!(
            state["P1"]["cemetery"],
            if returned {
                json!([])
            } else {
                json!(["source"])
            }
        );
        assert_eq!(state["objects"]["source"]["generation"], 2);
    }
}
