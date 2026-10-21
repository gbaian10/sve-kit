use super::*;
mod review_checks;

fn shared(kind: ZoneKind) -> MoveDestination {
    MoveDestination::Shared {
        zone: ZoneRef {
            player: PlayerRef::Controller {},
            kind,
        },
    }
}
fn movement(to: MoveDestination) -> MoveSpec {
    MoveSpec {
        to,
        position: None,
        initial_posture: None,
        entry_effects: vec![],
        order: None,
    }
}
fn moving(id: u64, objects: Selector, to: MoveDestination) -> Located<Node> {
    instruction(
        Action::Move {
            objects,
            movement: movement(to),
            reason: MoveReason::Put,
        },
        id,
        "synthetic",
    )
}
fn objects() -> Selector {
    Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    }
}
fn after(receipt: MovementReceipt) -> Selector {
    Selector::MoveAfter {
        receipt,
        permission: FollowPermission::SameEffectExplicitReference {},
    }
}
fn moved(id: u64) -> MovementReceipt {
    MovementReceipt::Move {
        receipt: ReceiptReference::new(ReceiptId(id)),
    }
}
fn reveal(id: u64, objects: Selector) -> Located<Node> {
    instruction(Action::Reveal { objects }, id, "synthetic")
}
fn action(node: &mut Located<Node>) -> &mut Action {
    let Node::Instruction { instruction } = &mut node.node else {
        panic!("instruction")
    };
    &mut instruction.action
}
fn search() -> Located<Node> {
    instruction(
        Action::Search {
            decision: DecisionId(9001),
            player: PlayerRef::Controller {},
            deck: PlayerRef::Controller {},
            candidate: Binding::new(1, ScopeId(2)),
            filter: cp04_filter(Binding::new(1, ScopeId(2))),
            cardinality: bounds(1, Some(1)),
            groups: None,
            destination: Some(SearchDestination::Single {
                movement: movement(shared(ZoneKind::Ex)),
            }),
            continuation: SearchContinuation {
                found: Binding::new(2, ScopeId(3)),
                revealed: Binding::new(3, ScopeId(3)),
            },
        },
        1,
        "CP04-060",
    )
}
fn install(id: u64, objects: Selector) -> Located<Node> {
    let affected = Binding::new(id, ScopeId(id.checked_add(1000).unwrap()));
    instruction(
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::CostModifier {
                subjects: Selector::Bound {
                    reference: affected,
                },
                change: CostChange::Delta {
                    amount: Expr::Literal(Int(-2)),
                },
                duration: turn(),
                usage: None,
            }),
            duration: turn(),
        },
        id,
        "CP04-060",
    )
}

#[test]
fn cp04_search_moves_are_the_only_cross_generation_discount_input() {
    let search_result = MovementReceipt::Search {
        receipt: ReceiptReference::new(ReceiptId(1)),
    };
    accept(body(
        1,
        vec![search(), install(2, after(search_result.clone()))],
    ));
    let cp04 = cp04(true, false)
        .node
        .abilities
        .remove(0)
        .node
        .body
        .unwrap();
    accept(cp04);
    for reference in [
        Binding::new(2, ScopeId(3)),
        Binding::new(3, ScopeId(3)),
        Binding::new(99, ScopeId(1)),
    ] {
        reject(
            body(1, vec![search(), install(2, Selector::Bound { reference })]),
            WellFormedReason::UnavailableBinding,
        );
    }
    reject(
        body(1, vec![install(2, after(search_result))]),
        WellFormedReason::UnavailableReceipt,
    );
    reject(
        body(1, vec![search(), install(2, after(moved(1)))]),
        WellFormedReason::ReceiptType,
    );
    reject(
        body(
            1,
            vec![
                search(),
                reveal(
                    2,
                    Selector::Objects {
                        objects: vec![ObjectOperand::Bound {
                            binding: Binding::new(1, ScopeId(2)),
                        }],
                    },
                ),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
}

#[test]
fn search_filter_and_continuation_are_disjoint_even_for_unexecuted_branches() {
    let mut step = search();
    if let Action::Search { destination, .. } = action(&mut step) {
        *destination = Some(SearchDestination::OptionalSelfReplacement {
            decision: DecisionId(9002),
            predicate: positive(Expr::Frozen(Box::new(NatFrozen::SearchFoundCount {
                found: Binding::new(2, ScopeId(3)),
            }))),
            movement: movement(shared(ZoneKind::Ex)),
        });
    }
    accept(body(1, vec![step.clone()]));
    if let Action::Search {
        destination: Some(SearchDestination::OptionalSelfReplacement { predicate, .. }),
        ..
    } = action(&mut step)
    {
        *predicate = positive(Expr::Frozen(Box::new(NatFrozen::SearchRevealedCount {
            revealed: Binding::new(3, ScopeId(3)),
        })));
    }
    accept(body(1, vec![step.clone()]));
    for value in [
        NatFrozen::SearchFoundCount {
            found: Binding::new(2, ScopeId(3)),
        },
        NatFrozen::SearchRevealedCount {
            revealed: Binding::new(3, ScopeId(3)),
        },
    ] {
        reject(
            body(
                1,
                vec![
                    step.clone(),
                    draw(2, signed(Expr::Frozen(Box::new(value.clone())))),
                ],
            ),
            WellFormedReason::UnavailableBinding,
        );
        let mut early = step.clone();
        if let Action::Search { filter, .. } = action(&mut early) {
            *filter = positive(Expr::Frozen(Box::new(value)));
        }
        let error = reject(body(1, vec![early]), WellFormedReason::UnavailableBinding);
        assert!(error.ir_path.ends_with("/action/filter"));
    }
    if let Action::Search {
        destination: Some(SearchDestination::OptionalSelfReplacement { predicate, .. }),
        ..
    } = action(&mut step)
    {
        *predicate = cp04_filter(Binding::new(1, ScopeId(2)));
    }
    let error = reject(
        body(
            1,
            vec![control(Control::If {
                condition: Expr::Literal(Bool(false)),
                then_body: body(5, vec![step]),
                else_body: None,
            })],
        ),
        WellFormedReason::UnavailableBinding,
    );
    assert!(error.ir_path.contains("/destination"));
}

#[test]
fn search_validates_every_destination_and_rejects_unsupported_choice_regions() {
    let valid_destination = Some(SearchDestination::Partition {
        destinations: vec![
            SearchPartition {
                decision: DecisionId(9010),
                zone: MoveDestination::Owners {
                    zone: OwnerZone::Hand,
                },
                cardinality: bounds(1, Some(1)),
            },
            SearchPartition {
                decision: DecisionId(9011),
                zone: MoveDestination::Owners {
                    zone: OwnerZone::Cemetery,
                },
                cardinality: bounds(1, Some(1)),
            },
        ],
    });
    let mut step = search();
    if let Action::Search {
        destination: to, ..
    } = action(&mut step)
    {
        *to = valid_destination;
    }
    accept(body(1, vec![step]));
    let mut invalid = search();
    if let Action::Search { destination, .. } = action(&mut invalid) {
        *destination = Some(SearchDestination::Partition {
            destinations: vec![
                SearchPartition {
                    decision: DecisionId(9012),
                    zone: MoveDestination::Owners {
                        zone: OwnerZone::Hand,
                    },
                    cardinality: bounds(1, Some(1)),
                },
                SearchPartition {
                    decision: DecisionId(9013),
                    zone: shared(ZoneKind::Cemetery),
                    cardinality: bounds(1, Some(1)),
                },
            ],
        });
    }
    let destination_error = reject(body(1, vec![invalid]), WellFormedReason::InvalidMovement);
    assert!(destination_error.ir_path.ends_with("/destination/1"));
    let mut empty = search();
    if let Action::Search { destination, .. } = action(&mut empty) {
        *destination = Some(SearchDestination::Partition {
            destinations: vec![],
        });
    }
    let empty_error = reject(body(1, vec![empty]), WellFormedReason::UnsupportedShape);
    assert!(empty_error.ir_path.ends_with("/action/destination"));
    let mut groups = search();
    if let Action::Search { groups, .. } = action(&mut groups) {
        *groups = Some(constraint(objects()));
    }
    reject(body(1, vec![groups]), WellFormedReason::UnsupportedShape);
    let mut choice = choose(1, 2);
    choice.kind = DecisionKind::ResolutionChoice {
        may: Declinable::MayDecline,
        cardinality: bounds(1, Some(1)),
        precondition: None,
    };
    choice.domain = ChoiceDomain::Compound { domains: vec![] };
    let choice_error = reject(
        body(
            1,
            vec![control(Control::May {
                choice,
                body: body(2, vec![search()]),
                receipt: ReceiptId(3),
            })],
        ),
        WellFormedReason::UnsupportedShape,
    );
    assert!(
        choice_error
            .ir_path
            .ends_with("/control/body/steps/0/instruction/action")
    );
}

#[test]
fn r4_1_uses_each_objects_owner_instead_of_the_source_controller() {
    let mixed = Selector::Union {
        sets: vec![
            zone(PlayerRef::Controller {}, ZoneKind::Field, 10),
            zone(PlayerRef::Active {}, ZoneKind::Field, 11),
        ],
    };
    for zone in [
        OwnerZone::Hand,
        OwnerZone::Deck,
        OwnerZone::Cemetery,
        OwnerZone::Banish,
    ] {
        accept(body(
            1,
            vec![moving(1, mixed.clone(), MoveDestination::Owners { zone })],
        ));
    }
    for zone in [
        ZoneKind::Hand,
        ZoneKind::Deck,
        ZoneKind::Cemetery,
        ZoneKind::Banish,
        ZoneKind::Evolve,
    ] {
        let error = reject(
            body(1, vec![moving(1, mixed.clone(), shared(zone))]),
            WellFormedReason::InvalidMovement,
        );
        assert!(error.ir_path.ends_with("/to"));
    }
    for zone in [ZoneKind::Field, ZoneKind::Ex] {
        accept(body(1, vec![moving(1, mixed.clone(), shared(zone))]));
    }
    let mut step = moving(
        1,
        objects(),
        MoveDestination::Owners {
            zone: OwnerZone::Deck,
        },
    );
    if let Action::Move { movement, .. } = action(&mut step) {
        movement.position = Some(Position::Top {});
    }
    accept(body(1, vec![step.clone()]));
    if let Action::Move { movement, .. } = action(&mut step) {
        movement.to = shared(ZoneKind::Ex);
    }
    reject(body(1, vec![step]), WellFormedReason::InvalidMovement);
    let mut posture_step = moving(1, objects(), shared(ZoneKind::Field));
    if let Action::Move { movement, .. } = action(&mut posture_step) {
        movement.initial_posture = Some(Posture::Acted);
    }
    accept(body(1, vec![posture_step.clone()]));
    if let Action::Move { movement, .. } = action(&mut posture_step) {
        movement.to = shared(ZoneKind::Ex);
    }
    reject(
        body(1, vec![posture_step]),
        WellFormedReason::InvalidMovement,
    );
}

#[test]
fn moved_receipts_preserve_producer_type_before_members_and_follow_permissions() {
    let cases = [
        (
            Action::Destroy { objects: objects() },
            MovementReceipt::Destroy {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
        ),
        (
            Action::Banish { objects: objects() },
            MovementReceipt::Banish {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
        ),
        (
            Action::Discard {
                player: PlayerRef::Controller {},
                mode: DiscardMode::DownTo { count: Nat(1) },
            },
            MovementReceipt::Discard {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
        ),
    ];
    for (producer, receipt) in cases {
        let producer = instruction(producer, 1, "synthetic");
        accept(body(
            1,
            vec![producer.clone(), reveal(2, after(receipt.clone()))],
        ));
        accept(body(
            1,
            vec![
                producer.clone(),
                reveal(
                    2,
                    Selector::ReceiptMembers {
                        receipt: ObjectReceipt::Movement {
                            receipt: receipt.clone(),
                        },
                        projection: MemberProjection::Before,
                    },
                ),
            ],
        ));
        let projection_error = reject(
            body(
                1,
                vec![
                    producer.clone(),
                    reveal(
                        2,
                        Selector::ReceiptMembers {
                            receipt: ObjectReceipt::Movement {
                                receipt: receipt.clone(),
                            },
                            projection: MemberProjection::After,
                        },
                    ),
                ],
            ),
            WellFormedReason::UnsupportedShape,
        );
        assert!(
            projection_error
                .ir_path
                .ends_with("/steps/1/instruction/action")
        );
        reject(
            body(1, vec![producer.clone(), reveal(2, after(moved(1)))]),
            WellFormedReason::ReceiptType,
        );
        let permission_error = reject(
            body(
                1,
                vec![
                    producer,
                    reveal(
                        2,
                        Selector::MoveAfter {
                            receipt,
                            permission: FollowPermission::TriggerExplicitReference {
                                event: EventContext::Trigger {},
                            },
                        },
                    ),
                ],
            ),
            WellFormedReason::UnsupportedShape,
        );
        assert!(
            permission_error
                .ir_path
                .ends_with("/steps/1/instruction/action")
        );
    }
}

#[test]
fn bp18_remainder_subtracts_successful_before_members_and_discount_uses_after() {
    let observed = Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        projection: MemberProjection::Before,
    };
    let look = instruction(
        Action::Look {
            viewer: PlayerRef::Controller {},
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(3)),
            mode: StepwiseMode::Exact,
        },
        1,
        "BP18-059",
    );
    let mut choice = chosen_objects(
        10,
        1,
        Selector::Filter {
            input: Box::new(observed.clone()),
            binding: Binding::new(20, ScopeId(20)),
            predicate: Box::new(Expr::Compute(Box::new(Predicate::HasTrait {
                traits: Expr::Live(Box::new(TraitsRead::Traits {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::Bound {
                            binding: Binding::new(20, ScopeId(20)),
                        }),
                    },
                    view: CharacteristicView::Effective,
                })),
                trait_id: TraitId("武闘竜人".into()),
            }))),
        },
    );
    choice.visibility = Visibility::Only {
        players: controller_set(),
    };
    let choice = decision(choice);
    let move_one = moving(
        2,
        Selector::Bound {
            reference: Binding::new(10, ScopeId(1)),
        },
        shared(ZoneKind::Ex),
    );
    let mut remainder = moving(
        3,
        Selector::Difference {
            input: Box::new(observed),
            except: Box::new(Selector::ReceiptMembers {
                receipt: ObjectReceipt::Movement { receipt: moved(2) },
                projection: MemberProjection::Before,
            }),
        },
        MoveDestination::Owners {
            zone: OwnerZone::Deck,
        },
    );
    if let Action::Move { movement, .. } = action(&mut remainder) {
        movement.position = Some(Position::Bottom {});
    }
    let mut discount = install(4, after(moved(2)));
    discount.source = source("BP18-059");
    if let Action::InstallEffect { effect, .. } = action(&mut discount)
        && let Effect::CostModifier { change, .. } = effect.as_mut()
    {
        *change = CostChange::Delta {
            amount: Expr::Literal(Int(-3)),
        };
    }
    accept(body(
        1,
        vec![
            look.clone(),
            choice.clone(),
            move_one.clone(),
            remainder.clone(),
            discount,
        ],
    ));
    reject(
        body(1, vec![look, choice, remainder, move_one]),
        WellFormedReason::UnavailableReceipt,
    );
}

#[test]
fn move_provenance_does_not_publish_hidden_cards_to_a_public_decision() {
    let search_error = reject(
        body(
            1,
            vec![
                search(),
                decision(chosen_objects(
                    2,
                    1,
                    after(MovementReceipt::Search {
                        receipt: ReceiptReference::new(ReceiptId(1)),
                    }),
                )),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
    assert!(
        search_error
            .ir_path
            .ends_with("/steps/1/decision/domain/groups/0")
    );
    let move_error = reject(
        body(
            1,
            vec![
                moving(1, objects(), shared(ZoneKind::Ex)),
                decision(chosen_objects(2, 1, after(moved(1)))),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
    assert!(
        move_error
            .ir_path
            .ends_with("/steps/1/decision/domain/groups/0")
    );
}

#[test]
fn search_bindings_and_cardinality_cannot_be_forged() {
    let mut wrong_scope = search();
    if let Action::Search { continuation, .. } = action(&mut wrong_scope) {
        continuation.revealed = Binding::new(3, ScopeId(4));
    }
    reject(body(1, vec![wrong_scope]), WellFormedReason::WrongScope);
    let mut same_slot = search();
    if let Action::Search { continuation, .. } = action(&mut same_slot) {
        continuation.revealed = Binding::new(2, ScopeId(3));
    }
    reject(body(1, vec![same_slot]), WellFormedReason::DuplicateBinding);
    let mut wrong_cardinality = search();
    if let Action::Search { cardinality, .. } = action(&mut wrong_cardinality) {
        *cardinality = bounds(2, Some(1));
    }
    reject(
        body(1, vec![wrong_cardinality]),
        WellFormedReason::InvalidCardinality,
    );
    let mut wrong_type = search();
    if let Action::Search { destination, .. } = action(&mut wrong_type) {
        *destination = Some(SearchDestination::OptionalSelfReplacement {
            decision: DecisionId(9002),
            predicate: positive(Expr::Frozen(Box::new(NatFrozen::SearchFoundCount {
                found: Binding::new(3, ScopeId(3)),
            }))),
            movement: movement(shared(ZoneKind::Ex)),
        });
    }
    reject(body(1, vec![wrong_type]), WellFormedReason::BindingType);
}

mod ordering;
mod search_groups;
mod search_inputs;
