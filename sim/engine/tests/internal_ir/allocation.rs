use super::*;
use sve_engine::game::internal::{DamageShare, DecisionToken, InputDomain};

fn distribution(
    id: u64,
    recipients: RecipientSet,
    total: Expr<Nat>,
    minimum: u64,
) -> Located<Node> {
    node(Node::Decision {
        decision: Decision {
            id: DecisionId(id),
            actor: PlayerRef::Controller {},
            visibility: Visibility::Public {},
            captures: vec![],
            domain: ChoiceDomain::Distribution {
                recipients,
                total,
                minimum: Nat(minimum),
                binding: Binding::new(id, ScopeId(1)),
            },
            kind: DecisionKind::ResolutionChoice {
                may: Declinable::Required,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                precondition: None,
            },
        },
    })
}

fn allocated(id: u64, producer: u64, recipients: RecipientSet) -> Located<Node> {
    instruction(
        id,
        Action::Damage {
            recipients,
            amount: DamageAmount::Allocation {
                binding: Binding::new(producer, ScopeId(1)),
            },
            flags: DamageFlags::Ability,
            source: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            batch: BatchPolicy::Instruction {},
        },
    )
}

fn both_fields() -> RecipientSet {
    RecipientSet::Objects {
        objects: zone(ZoneKind::Field, 70),
    }
}

const fn leaders() -> RecipientSet {
    RecipientSet::Leaders {
        players: players(PlayerRelation::All),
    }
}

fn answer(values: &[(u64, u64)]) -> Answer {
    Answer::Distribution(
        values
            .iter()
            .map(|&(handle, amount)| DamageShare {
                handle,
                amount: Nat(amount),
            })
            .collect(),
    )
}

fn result(progress: &Progress, id: u64, requested: i64, actual: u64) {
    assert_eq!(
        progress.receipts.get(&ReceiptId(id)),
        Some(&InstructionOutcome::Damage(DamageQuantityOutcome {
            requested: Int(requested),
            actual: Nat(actual)
        })),
        "allocation receipt {id}"
    );
}

fn reject(game: &mut Game, actor: PlayerId, token: DecisionToken, answer: Answer, expected: &str) {
    let before = exact_authority(game);
    let views = [View::P1, View::P2, View::Referee];
    let observations = views.map(|view| game.projection(view).unwrap());
    let pending = game.internal_decision(View::Referee);
    let error = game.answer_internal(actor, token, answer).unwrap_err();
    assert!(error.to_string().contains(expected), "{error}");
    assert_eq!(
        exact_authority(game),
        before,
        "invalid answer must preserve authority"
    );
    assert_eq!(
        game.internal_decision(View::Referee),
        pending,
        "invalid answer preserves the question"
    );
    assert_eq!(
        views.map(|view| game.projection(view).unwrap()),
        observations,
        "invalid answer preserves all observations"
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "The full rejection matrix shares one immutable pending question."
)]
fn complete_answers_minima_exact_total_actor_epoch_and_handles_both_seats() {
    for seat in [1, 2] {
        for (minimum, values) in [
            (0, vec![(0, 4), (1, 0)]),
            (1, vec![(0, 3), (1, 1)]),
            (2, vec![(1, 2), (0, 2)]),
        ] {
            let mut game = game(seat, 0);
            let recipients = leaders();
            let program = checked(vec![
                distribution(1, recipients.clone(), Expr::Literal(Nat(4)), minimum),
                allocated(2, 1, recipients),
            ]);
            let progress = game
                .execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
            assert!(
                !progress.complete && progress.facts.is_empty() && progress.receipts.is_empty()
            );
            let question = game.internal_decision(View::Referee).unwrap();
            let InputDomain::Distribution(domain) = &question.domain else {
                panic!("distribution")
            };
            assert_eq!(domain.total, Nat(4));
            assert_eq!(domain.minimum, Nat(minimum));
            assert_eq!(domain.members.len(), 2);
            let other = if seat == 1 { View::P2 } else { View::P1 };
            assert!(game.internal_decision(other).is_none());
            assert!(game.projection(other).unwrap()["awaiting"].is_null());
            reject(
                &mut game,
                PlayerId {
                    seat: if seat == 1 { 2 } else { 1 },
                },
                question.token,
                answer(&values),
                "wrong actor",
            );
            for bad in [
                DecisionToken {
                    epoch: question.token.epoch + 1,
                    ..question.token
                },
                DecisionToken {
                    execution: ExecutionId(question.token.execution.0 + 1),
                    ..question.token
                },
                DecisionToken {
                    decision: DecisionId(999),
                    ..question.token
                },
            ] {
                reject(
                    &mut game,
                    question.actor,
                    bad,
                    answer(&values),
                    "stale internal answer",
                );
            }
            for invalid in [
                vec![],
                vec![(0, 4)],
                vec![(0, 2), (0, 2)],
                vec![(0, 2), (99, 2)],
                vec![(0, 2), (1, 3)],
                vec![(0, u64::MAX), (1, 1)],
            ] {
                reject(
                    &mut game,
                    question.actor,
                    question.token,
                    answer(&invalid),
                    "distribution",
                );
            }
            if minimum > 0 {
                reject(
                    &mut game,
                    question.actor,
                    question.token,
                    answer(&[(0, 4), (1, 0)]),
                    "bounds",
                );
            }
            reject(
                &mut game,
                question.actor,
                question.token,
                Answer::Integer(Nat(4)),
                "outside the internal domain",
            );
            let progress = game
                .answer_internal(question.actor, question.token, answer(&values))
                .unwrap();
            assert!(progress.complete);
            result(&progress, 2, 4, 4);
            let hits = progress
                .facts
                .iter()
                .filter_map(|fact| {
                    if let Fact::Damage(hit) = fact {
                        Some(hit.amount.0.get())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(hits.len(), if minimum == 0 { 1 } else { 2 });
            assert_eq!(hits.iter().sum::<u64>(), 4);
            reject(
                &mut game,
                question.actor,
                question.token,
                answer(&values),
                "no internal input point",
            );
        }
    }
}

#[test]
fn empty_domain_skips_distribution_and_preserves_zero_receipt_then_later_damage() {
    for seat in [1, 2] {
        let mut game = game(seat, 0);
        let empty = RecipientSet::Explicit { recipients: vec![] };
        let progress = game
            .execute_internal(
                &checked(vec![
                    distribution(1, empty.clone(), Expr::Literal(Nat(u64::MAX)), u64::MAX),
                    allocated(2, 1, empty),
                    damage(
                        3,
                        RecipientSet::Leaders {
                            players: players(PlayerRelation::Opponents),
                        },
                        5,
                    ),
                ]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        assert!(progress.complete && game.internal_decision(View::Referee).is_none());
        result(&progress, 2, 0, 0);
        result(&progress, 3, 5, 5);
        assert!(matches!(progress.facts.as_slice(), [Fact::Damage(_)]));
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One resolution demonstrates both frozen membership and frozen quantity."
)]
fn frozen_union_members_and_total_ignore_later_live_changes_and_keep_original_generations() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let mut position = setup(
            seat,
            json!(
                (0..12)
                    .map(|i| json!({"id":format!("d{i}"),"card":"unit-follower"}))
                    .collect::<Vec<_>>()
            ),
        );
        position["players"][actor]["leader"]["life"] = json!(10);
        position["players"][actor]["zones"]["field"] = json!([{"id":"a","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "frozen-allocation",
        )
        .unwrap();
        let recipients = RecipientSet::Union {
            sets: vec![
                both_fields(),
                RecipientSet::Objects {
                    objects: zone(ZoneKind::Field, 71),
                },
            ],
        };
        let total = Expr::Compute(Box::new(NatOperation::CheckedInt {
            value: Expr::Live(Box::new(IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::Life,
            })),
        }));
        let own = own_field(72);
        let program = checked(vec![
            distribution(1, recipients.clone(), total, 1),
            movement::put(2, own, OwnerZone::Banish),
            damage(
                3,
                RecipientSet::Leaders {
                    players: players(PlayerRelation::SelfPlayer),
                },
                1,
            ),
            allocated(4, 1, recipients),
            draw_value(5, requested_damage(4)),
            draw_value(
                6,
                Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Frozen(Box::new(NatFrozen::ReceiptActual {
                        receipt: MeasuredReceipt::Damage {
                            receipt: ReceiptReference::new(ReceiptId(4)),
                        },
                    })),
                })),
            ),
        ]);
        game.execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        let question = game.internal_decision(View::Referee).unwrap();
        let InputDomain::Distribution(domain) = &question.domain else {
            panic!("distribution")
        };
        assert_eq!(domain.members.len(), 2);
        assert_eq!(domain.total, Nat(10));
        let own = domain
            .members
            .iter()
            .find(|member| member.player.seat == seat)
            .unwrap()
            .handle;
        let other = domain
            .members
            .iter()
            .find(|member| member.player.seat != seat)
            .unwrap()
            .handle;
        let progress = game
            .answer_internal(
                question.actor,
                question.token,
                answer(&[(own, 9), (other, 1)]),
            )
            .unwrap();
        result(&progress, 4, 10, 1);
        cards(&progress, 5, 10, 10);
        cards(&progress, 6, 1, 1);
        let hits = progress
            .facts
            .iter()
            .filter_map(|fact| {
                if let Fact::Damage(hit) = fact {
                    Some(hit)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[1].amount.0.get(), 1);
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection[actor]["leader"]["life"], 9);
        assert_eq!(projection["objects"]["f"]["hp"], 2);
    }
}

fn own_field(scope: u64) -> Selector {
    Selector::Zone {
        players: players(PlayerRelation::SelfPlayer),
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding: Binding::new(9, ScopeId(scope)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn requested_damage(id: u64) -> Expr<Int> {
    Expr::Frozen(Box::new(IntFrozen::ReceiptRequested {
        receipt: MeasuredReceipt::Damage {
            receipt: ReceiptReference::new(ReceiptId(id)),
        },
    }))
}

#[test]
fn infeasible_or_overflowing_distribution_restores_prior_draws_and_observations() {
    for seat in [1, 2] {
        for (total, minimum, expected) in [
            (3, 2, "infeasible distribution"),
            (4, u64::MAX, "minimum overflow"),
            (u64::MAX, 0, "receipt range"),
        ] {
            let mut game = game(seat, 2);
            let before = exact_authority(&game);
            let views = [View::P1, View::P2, View::Referee];
            let observations = views.map(|view| game.projection(view).unwrap());
            let error = game
                .execute_internal(
                    &checked(vec![
                        draw(1, 1),
                        distribution(2, leaders(), Expr::Literal(Nat(total)), minimum),
                        draw(3, 1),
                    ]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
            assert_eq!(exact_authority(&game), before);
            assert_eq!(
                views.map(|view| game.projection(view).unwrap()),
                observations
            );
        }
    }
}

#[test]
fn a_later_infeasible_domain_rolls_back_entire_execution_and_revokes_observed_tokens() {
    for seat in [1, 2] {
        let mut game = game(seat, 2);
        let views = [View::P1, View::P2, View::Referee];
        let observations = views.map(|view| game.projection(view).unwrap());
        let saved_before = serde_json::to_value(&game).unwrap();
        let program = checked(vec![
            draw(1, 1),
            distribution(2, leaders(), Expr::Literal(Nat(4)), 1),
            allocated(3, 2, leaders()),
            distribution(4, leaders(), Expr::Literal(Nat(3)), 2),
        ]);
        game.execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        let old = game.internal_decision(View::Referee).unwrap();
        let error = game
            .answer_internal(old.actor, old.token, answer(&[(0, 3), (1, 1)]))
            .unwrap_err();
        assert!(error.to_string().contains("infeasible distribution"));
        assert!(game.internal_decision(View::Referee).is_none());
        assert_eq!(
            views.map(|view| game.projection(view).unwrap()),
            observations
        );
        let mut saved_after = serde_json::to_value(&game).unwrap();
        assert!(
            saved_after["state"]["ir_epoch"].as_u64().unwrap()
                > saved_before["state"]["ir_epoch"].as_u64().unwrap()
        );
        saved_after["state"]["ir_epoch"] = saved_before["state"]["ir_epoch"].clone();
        assert_eq!(saved_after, saved_before);
        game.execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        let fresh = game.internal_decision(View::Referee).unwrap();
        assert!(fresh.token.epoch > old.token.epoch);
        assert_ne!(fresh.token.execution, old.token.execution);
        reject(
            &mut game,
            fresh.actor,
            old.token,
            answer(&[(0, 3), (1, 1)]),
            "stale internal answer",
        );
    }
}

#[test]
fn allocation_zero_and_overkill_do_not_cap_at_life_or_emit_zero_facts() {
    for seat in [1, 2] {
        let mut game = game(seat, 0);
        game.execute_internal(
            &checked(vec![
                distribution(1, leaders(), Expr::Literal(Nat(30)), 0),
                allocated(2, 1, leaders()),
            ]),
            &source("synthetic"),
            context(seat),
        )
        .unwrap();
        let question = game.internal_decision(View::Referee).unwrap();
        let progress = game
            .answer_internal(question.actor, question.token, answer(&[(0, 30), (1, 0)]))
            .unwrap();
        result(&progress, 2, 30, 30);
        assert!(
            matches!(progress.facts.as_slice(),[Fact::Damage(hit)] if hit.amount.0.get() == 30)
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            -10
        );
    }
}

#[test]
fn real_resolution_without_timing_evidence_and_dead_unsupported_shapes_reject_before_draw() {
    for seat in [1, 2] {
        let mut game = game(seat, 1);
        let before = exact_authority(&game);
        let program = checked(vec![
            draw(1, 1),
            distribution(2, leaders(), Expr::Literal(Nat(4)), 1),
        ]);
        let error = game
            .execute_internal(&program, &source("BP08-028"), context(seat))
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("reviewed timing/minimum evidence")
        );
        assert_eq!(exact_authority(&game), before);
        let mut choice = distribution(2, leaders(), Expr::Literal(Nat(4)), 1);
        let Node::Decision { decision } = &mut choice.node else {
            panic!("fixture shape")
        };
        decision.visibility = Visibility::AuthorityOnly {};
        let ChoiceDomain::Distribution { binding, .. } = &mut decision.domain else {
            panic!("distribution")
        };
        *binding = Binding::new(2, ScopeId(2));
        let program = checked(vec![
            draw(1, 1),
            node(Node::Control {
                control: Control::If {
                    condition: Expr::Literal(Bool(false)),
                    then_body: body(2, vec![choice]),
                    else_body: None,
                },
            }),
        ]);
        let error = game
            .execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap_err();
        assert!(error.to_string().contains("AuthorityOnly"), "{error}");
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn non_follower_and_off_field_distribution_domains_reject_before_asking() {
    for seat in [1, 2] {
        for (card, kind, expected) in [
            (
                "unit-amulet",
                ZoneKind::Field,
                "non-follower distribution recipient",
            ),
            (
                "unit-follower",
                ZoneKind::Cemetery,
                "non-current-field distribution recipient",
            ),
        ] {
            let actor = if seat == 1 { "P1" } else { "P2" };
            let mut position = setup(seat, json!([{ "id":"d","card":"unit-follower" }]));
            let zone_name = if kind == ZoneKind::Field {
                "field"
            } else {
                "cemetery"
            };
            position["players"][actor]["zones"][zone_name] = json!([{ "id":"a","card":card }]);
            let mut game = Game::new(
                catalog(),
                &position,
                &Value::Null,
                &Value::Null,
                "rejected-domain",
            )
            .unwrap();
            let before = exact_authority(&game);
            let recipients = RecipientSet::Objects {
                objects: zone(kind, 75),
            };
            let error = game
                .execute_internal(
                    &checked(vec![
                        draw(1, 1),
                        distribution(2, recipients, Expr::Literal(Nat(4)), 0),
                    ]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
            assert_eq!(exact_authority(&game), before);
        }
    }
}

#[test]
fn five_frozen_targets_cannot_share_four_with_minimum_one() {
    for seat in [1, 2] {
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut position = setup(seat, json!([]));
        position["players"][opponent]["zones"]["field"] = json!(
            (0..5)
                .map(|i| json!({"id":format!("f{i}"),"card":"unit-follower"}))
                .collect::<Vec<_>>()
        );
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "five-targets",
        )
        .unwrap();
        let before = exact_authority(&game);
        let error = game
            .execute_internal(
                &checked(vec![distribution(
                    1,
                    both_fields(),
                    Expr::Literal(Nat(4)),
                    1,
                )]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("infeasible distribution minimum")
        );
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn successive_executions_never_reuse_allocation_answers_or_tokens() {
    for seat in [1, 2] {
        let mut game = game(seat, 0);
        let program = checked(vec![
            distribution(1, leaders(), Expr::Literal(Nat(4)), 1),
            allocated(2, 1, leaders()),
        ]);
        game.execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        let first = game.internal_decision(View::Referee).unwrap();
        game.answer_internal(first.actor, first.token, answer(&[(0, 3), (1, 1)]))
            .unwrap();
        let progress = game
            .execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        assert!(progress.receipts.is_empty() && progress.facts.is_empty());
        let second = game.internal_decision(View::Referee).unwrap();
        assert_ne!(first.token.execution, second.token.execution);
        assert!(second.token.epoch > first.token.epoch);
        reject(
            &mut game,
            second.actor,
            first.token,
            answer(&[(0, 3), (1, 1)]),
            "stale internal answer",
        );
        let progress = game
            .answer_internal(second.actor, second.token, answer(&[(0, 1), (1, 3)]))
            .unwrap();
        result(&progress, 2, 4, 4);
    }
}

#[test]
fn mixed_interactive_continuations_reject_whole_trees_including_dead_branches() {
    let empty = Selector::Objects { objects: vec![] };
    let interactive = [
        up_to(3, 1),
        instruction(
            3,
            Action::Order {
                decision: DecisionId(3),
                cards: empty.clone(),
                destinations: vec![Placement {
                    cards: empty,
                    zone: MoveDestination::Owners {
                        zone: OwnerZone::Deck,
                    },
                    position: Position::Top {},
                }],
                chooser: PlayerRef::Controller {},
                visibility: Visibility::Only {
                    players: players(PlayerRelation::SelfPlayer),
                },
            },
        ),
        instruction(
            3,
            Action::Search {
                decision: DecisionId(10),
                player: PlayerRef::Controller {},
                deck: PlayerRef::Controller {},
                candidate: Binding::new(1, ScopeId(2)),
                filter: Expr::Literal(Bool(true)),
                cardinality: Cardinality {
                    minimum: Nat(0),
                    maximum: Some(Nat(1)),
                },
                groups: None,
                destination: Some(SearchDestination::Single {
                    movement: MoveSpec {
                        to: MoveDestination::Owners {
                            zone: OwnerZone::Hand,
                        },
                        initial_posture: None,
                        entry_effects: vec![],
                        position: None,
                        order: None,
                    },
                }),
                continuation: SearchContinuation {
                    found: Binding::new(2, ScopeId(3)),
                    revealed: Binding::new(3, ScopeId(3)),
                },
            },
        ),
    ];
    for seat in [1, 2] {
        for step in &interactive {
            for dead in [false, true] {
                let step = if dead {
                    node(Node::Control {
                        control: Control::If {
                            condition: Expr::Literal(Bool(false)),
                            then_body: body(4, vec![step.clone()]),
                            else_body: None,
                        },
                    })
                } else {
                    step.clone()
                };
                let mut game = game(seat, 2);
                let before = exact_authority(&game);
                let observations =
                    [View::P1, View::P2, View::Referee].map(|view| game.projection(view).unwrap());
                let error = game
                    .execute_internal(
                        &checked(vec![
                            draw(1, 1),
                            distribution(2, leaders(), Expr::Literal(Nat(4)), 1),
                            step,
                        ]),
                        &source("synthetic"),
                        context(seat),
                    )
                    .unwrap_err();
                assert!(matches!(&error, sve_engine::EngineFailure::Unsupported(_)));
                assert!(
                    error
                        .to_string()
                        .contains("distribution mixed with another interactive continuation"),
                    "{error}"
                );
                assert_eq!(exact_authority(&game), before);
                assert_eq!(
                    [View::P1, View::P2, View::Referee].map(|view| game.projection(view).unwrap()),
                    observations
                );
            }
        }
    }
}
