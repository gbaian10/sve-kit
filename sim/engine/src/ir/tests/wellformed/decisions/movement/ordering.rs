use super::*;

fn private() -> Visibility {
    Visibility::Only {
        players: controller_set(),
    }
}

#[test]
fn order_reserves_a_decision_identity_in_the_shared_namespace() {
    let choice = decision(chosen_objects(30, 1, objects()));
    let ordering = order(objects(), Visibility::Public {});
    for steps in [
        vec![choice.clone(), ordering.clone()],
        vec![ordering.clone(), choice],
    ] {
        reject(body(1, steps), WellFormedReason::DuplicateDecision);
    }
    let mut second = ordering.clone();
    if let Node::Instruction { instruction } = &mut second.node {
        instruction.id = InstructionId(4);
        instruction.receipt = ReceiptId(4);
    }
    reject(
        body(1, vec![ordering, second]),
        WellFormedReason::DuplicateDecision,
    );
    accept(body(
        1,
        vec![
            decision(chosen_objects(20, 1, objects())),
            order(objects(), Visibility::Public {}),
        ],
    ));
}

#[test]
fn order_answers_are_complete_generation_sensitive_permutations() {
    let first = ObjectRef {
        id: ObjectId(1),
        generation: Generation(1),
    };
    let second = ObjectRef {
        id: ObjectId(2),
        generation: Generation(1),
    };
    let next = ObjectRef {
        generation: Generation(2),
        ..first
    };
    assert!(OrderAnswer { cards: vec![] }.matches_domain(&[]));
    for cards in [vec![first, second], vec![second, first]] {
        assert!(OrderAnswer { cards }.matches_domain(&[first, second]));
    }
    for cards in [vec![], vec![first], vec![first, first], vec![next, second]] {
        assert!(!OrderAnswer { cards }.matches_domain(&[first, second]));
    }
    assert!(!OrderAnswer { cards: vec![first] }.matches_domain(&[first, first]));
    assert!(!OrderAnswer { cards: vec![first] }.matches_domain(&[]));
}
fn order(cards: Selector, visibility: Visibility) -> Located<Node> {
    instruction(
        Action::Order {
            decision: DecisionId(30),
            cards: cards.clone(),
            destinations: vec![Placement {
                cards,
                zone: MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                },
                position: Position::Bottom {},
            }],
            chooser: PlayerRef::Controller {},
            visibility,
        },
        3,
        "BP07-SL09",
    )
}
pub(super) fn order_decision(objects: Selector, visibility: Visibility) -> Decision {
    Decision {
        domain: ChoiceDomain::Order {
            objects,
            binding: Binding::new(20, ScopeId(1)),
        },
        kind: DecisionKind::ResolutionChoice {
            may: Declinable::Required,
            cardinality: bounds(1, Some(1)),
            precondition: None,
        },
        visibility,
        ..choose(20, 1)
    }
}
fn ordered_move(cards: Selector, visibility: Visibility) -> Located<Node> {
    let mut step = moving(
        3,
        cards.clone(),
        MoveDestination::Owners {
            zone: OwnerZone::Deck,
        },
    );
    if let Action::Move { movement, .. } = action(&mut step) {
        movement.position = Some(Position::Bottom {});
        movement.order = Some(order_decision(cards, visibility));
    }
    step
}
fn looked() -> Selector {
    Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        projection: MemberProjection::Before,
    }
}
fn look() -> Located<Node> {
    instruction(
        Action::Look {
            viewer: PlayerRef::Controller {},
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(5)),
            mode: StepwiseMode::Exact,
        },
        1,
        "BP07-SL09",
    )
}

#[test]
fn bp07_sl09_remainder_uses_successful_before_and_keeps_private_order() {
    let chosen = Binding::new(10, ScopeId(1));
    let mut choice = chosen_objects(10, 1, looked());
    choice.visibility = private();
    let moved_cards = Selector::Bound { reference: chosen };
    let remainder = Selector::Difference {
        input: Box::new(looked()),
        except: Box::new(Selector::ReceiptMembers {
            receipt: ObjectReceipt::Movement { receipt: moved(2) },
            projection: MemberProjection::Before,
        }),
    };
    let prefix = vec![
        look(),
        decision(choice),
        moving(2, moved_cards, shared(ZoneKind::Field)),
    ];
    for last in [
        order(remainder.clone(), private()),
        ordered_move(remainder.clone(), private()),
    ] {
        let mut steps = prefix.clone();
        steps.push(last);
        accept(body(1, steps));
    }
    let mut public = prefix.clone();
    public.push(order(remainder, Visibility::Public {}));
    reject(body(1, public), WellFormedReason::UnsupportedShape);
    let mut after_steps = prefix;
    after_steps.push(order(after(moved(2)), private()));
    reject(body(1, after_steps), WellFormedReason::UnsupportedShape);
}

#[test]
fn order_does_not_publish_hidden_answers_and_has_a_typed_movement_receipt() {
    accept(body(
        1,
        vec![
            look(),
            order(looked(), private()),
            reveal(
                4,
                after(MovementReceipt::Order {
                    receipt: ReceiptReference::new(ReceiptId(3)),
                }),
            ),
        ],
    ));
    reject(
        body(
            1,
            vec![
                look(),
                order(looked(), private()),
                reveal(4, after(moved(3))),
            ],
        ),
        WellFormedReason::ReceiptType,
    );
    reject(
        body(
            1,
            vec![
                look(),
                ordered_move(looked(), private()),
                reveal(
                    4,
                    Selector::Bound {
                        reference: Binding::new(20, ScopeId(1)),
                    },
                ),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
    reject(
        body(1, vec![look(), order(looked(), Visibility::Public {})]),
        WellFormedReason::UnsupportedShape,
    );
    reject(
        body(
            1,
            vec![order(
                zone(PlayerRef::Controller {}, ZoneKind::Deck, 10),
                private(),
            )],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn ordering_rejects_foreign_domains_destinations_and_extra_input_points() {
    for change in 0_u8..7 {
        let mut step = ordered_move(looked(), private());
        if let Action::Move { movement, .. } = action(&mut step) {
            let decision = movement.order.as_mut().unwrap();
            match change {
                0 => {
                    movement.to = MoveDestination::Owners {
                        zone: OwnerZone::Hand,
                    }
                }
                1 => movement.position = None,
                2 => {
                    decision.domain = ChoiceDomain::Order {
                        objects: objects(),
                        binding: Binding::new(20, ScopeId(1)),
                    }
                }
                3 => decision.actor = PlayerRef::Owner {},
                4 => {
                    decision.kind = DecisionKind::ResolutionChoice {
                        may: Declinable::MayDecline,
                        cardinality: bounds(1, Some(1)),
                        precondition: None,
                    }
                }
                5 => {
                    decision.domain = ChoiceDomain::Integer {
                        cardinality: bounds(0, Some(1)),
                        binding: Binding::new(20, ScopeId(1)),
                    }
                }
                _ => {
                    decision.kind = DecisionKind::ResolutionChoice {
                        may: Declinable::Required,
                        cardinality: bounds(0, Some(1)),
                        precondition: None,
                    }
                }
            }
        }
        let (reason, suffix) = match change {
            0 => (WellFormedReason::InvalidMovement, "/to"),
            1 => (WellFormedReason::InvalidMovement, "/action"),
            4 | 6 => (WellFormedReason::DecisionTiming, "/order"),
            _ => (WellFormedReason::UnsupportedShape, "/order"),
        };
        let error = reject(body(1, vec![look(), step]), reason);
        assert!(error.ir_path.ends_with(suffix), "{error}");
    }
    for change in 0_u8..5 {
        let mut step = order(looked(), private());
        if let Action::Order {
            destinations,
            chooser,
            ..
        } = action(&mut step)
        {
            match change {
                0 => destinations.clear(),
                1 => destinations.push(destinations[0].clone()),
                2 => destinations[0].cards = objects(),
                3 => destinations[0].zone = shared(ZoneKind::Field),
                _ => *chooser = PlayerRef::Owner {},
            }
        }
        let reason = match change {
            2 | 3 => WellFormedReason::InvalidMovement,
            _ => WellFormedReason::UnsupportedShape,
        };
        let error = reject(body(1, vec![look(), step]), reason);
        assert!(error.ir_path.ends_with("/action"), "{error}");
    }
    let batch = simultaneous(vec![order(objects(), Visibility::Public {})], 9);
    reject(body(1, vec![batch]), WellFormedReason::UnsupportedShape);
}

#[test]
fn ordered_move_checks_candidate_domains_once_and_forbids_batch_prompts() {
    for cards in [
        zone(PlayerRef::Controller {}, ZoneKind::Field, 10),
        filtered(objects()),
    ] {
        accept(body(1, vec![ordered_move(cards, Visibility::Public {})]));
    }
    let hand = zone(PlayerRef::Controller {}, ZoneKind::Hand, 10);
    accept(body(1, vec![ordered_move(hand.clone(), private())]));
    let error = reject(
        body(1, vec![ordered_move(hand, Visibility::Public {})]),
        WellFormedReason::UnsupportedShape,
    );
    assert!(error.ir_path.ends_with("/order"), "{error}");
    let batch = simultaneous(vec![ordered_move(objects(), Visibility::Public {})], 9);
    let batch_error = reject(body(1, vec![batch]), WellFormedReason::UnsupportedShape);
    assert!(batch_error.ir_path.ends_with("/order"), "{batch_error}");
    accept(body(1, vec![look(), order(filtered(looked()), private())]));
}

fn filtered(input: Selector) -> Selector {
    Selector::Filter {
        input: Box::new(input),
        binding: Binding::new(30, ScopeId(30)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

#[test]
fn movement_before_evidence_precedes_candidate_definitions_and_keeps_its_audience() {
    for cards in [
        zone(PlayerRef::Controller {}, ZoneKind::Field, 10),
        filtered(objects()),
    ] {
        for banish in [false, true] {
            let producer = if banish {
                Action::Banish {
                    objects: cards.clone(),
                }
            } else {
                Action::Destroy {
                    objects: cards.clone(),
                }
            };
            let receipt = if banish {
                MovementReceipt::Banish {
                    receipt: ReceiptReference::new(ReceiptId(1)),
                }
            } else {
                MovementReceipt::Destroy {
                    receipt: ReceiptReference::new(ReceiptId(1)),
                }
            };
            let before = Selector::ReceiptMembers {
                receipt: ObjectReceipt::Movement { receipt },
                projection: MemberProjection::Before,
            };
            accept(body(
                1,
                vec![
                    instruction(producer, 1, "synthetic"),
                    decision(chosen_objects(2, 1, before)),
                ],
            ));
        }
    }
    let cards = zone(PlayerRef::Controller {}, ZoneKind::Hand, 10);
    let before = Selector::ReceiptMembers {
        receipt: ObjectReceipt::Movement { receipt: moved(1) },
        projection: MemberProjection::Before,
    };
    let producer = moving(1, cards, shared(ZoneKind::Ex));
    let mut choice = chosen_objects(2, 1, before);
    choice.visibility = private();
    accept(body(1, vec![producer.clone(), decision(choice.clone())]));
    choice.actor = PlayerRef::Owner {};
    choice.visibility = Visibility::Only {
        players: PlayerSet {
            relative_to: PlayerRef::Owner {},
            ..controller_set()
        },
    };
    let error = reject(
        body(1, vec![producer, decision(choice)]),
        WellFormedReason::UnsupportedShape,
    );
    assert!(error.ir_path.ends_with("/domain/groups/0"), "{error}");
}

#[test]
fn batch_publishes_before_evidence_only_after_all_siblings_finish() {
    let producer = moving(2, filtered(looked()), shared(ZoneKind::Field));
    let before = Selector::ReceiptMembers {
        receipt: ObjectReceipt::Movement { receipt: moved(2) },
        projection: MemberProjection::Before,
    };
    let remainder = Selector::Difference {
        input: Box::new(looked()),
        except: Box::new(before.clone()),
    };
    accept(body(
        1,
        vec![
            look(),
            simultaneous(vec![producer.clone()], 9),
            order(remainder, private()),
        ],
    ));
    let error = reject(
        body(
            1,
            vec![look(), simultaneous(vec![producer, reveal(4, before)], 9)],
        ),
        WellFormedReason::UnavailableReceipt,
    );
    assert!(error.ir_path.ends_with("/members/1/action"), "{error}");
}
