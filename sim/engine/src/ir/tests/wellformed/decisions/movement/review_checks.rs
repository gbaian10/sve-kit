use super::*;

#[test]
fn bp03_002_search_found_cost_is_local_to_the_destination_question() {
    let found = Binding::new(2, ScopeId(3));
    let object = Binding::new(1, ScopeId(4));
    let condition = Expr::Compute(Box::new(Predicate::SearchFoundAny {
        found,
        object,
        predicate: Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                information: NumberRead::Cost {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::Bound { binding: object }),
                    },
                    view: CharacteristicView::Effective,
                },
            })),
            comparison: Comparison::LessEqual,
            right: Expr::Literal(Int(2)),
        })),
    }));
    let mut node = search();
    if let Action::Search { destination, .. } = action(&mut node) {
        *destination = Some(SearchDestination::OptionalSelfReplacement {
            decision: DecisionId(9002),
            predicate: condition.clone(),
            movement: movement(shared(ZoneKind::Field)),
        });
    }
    accept(body(1, vec![node.clone()]));
    reject(
        body(
            1,
            vec![
                node.clone(),
                control(Control::If {
                    condition,
                    then_body: body(5, vec![]),
                    else_body: None,
                }),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
    if let Action::Search {
        destination: Some(SearchDestination::OptionalSelfReplacement { predicate, .. }),
        ..
    } = action(&mut node)
    {
        *predicate = Expr::Compute(Box::new(Predicate::SearchFoundAny {
            found: Binding::new(3, ScopeId(3)),
            object,
            predicate: Expr::Literal(Bool(true)),
        }));
    }
    reject(body(1, vec![node]), WellFormedReason::BindingType);
}

fn reject_action(program: Program, reason: WellFormedReason) {
    let error = reject(program, reason);
    assert!(
        error.ir_path.ends_with("/instruction/action"),
        "{}",
        error.ir_path
    );
}

#[test]
fn search_actor_and_installation_subject_regions_are_explicit() {
    let mut mismatched_actor = search();
    if let Node::Instruction { instruction } = &mut mismatched_actor.node {
        instruction.actor = PlayerRef::Active {};
    }
    let error = reject(
        body(1, vec![mismatched_actor]),
        WellFormedReason::UnsupportedShape,
    );
    assert!(error.ir_path.ends_with("/steps/0/instruction"));
    let mut reused_scope = search();
    if let Action::Search { continuation, .. } = action(&mut reused_scope) {
        continuation.found = Binding::new(2, ScopeId(1));
        continuation.revealed = Binding::new(3, ScopeId(1));
    }
    reject_action(
        body(1, vec![reused_scope]),
        WellFormedReason::DuplicateScope,
    );
    let mut installing_in_parent = install(1, objects());
    if let Action::InstallEffect {
        subjects: InstallationSubjects::Objects { affected, .. },
        ..
    } = action(&mut installing_in_parent)
    {
        *affected = Binding::new(1, ScopeId(1));
    }
    reject_action(
        body(1, vec![installing_in_parent]),
        WellFormedReason::DuplicateScope,
    );
    let mut players = install(1, objects());
    if let Action::InstallEffect { subjects, .. } = action(&mut players) {
        *subjects = InstallationSubjects::Players {
            players: controller_set(),
            affected: Binding::new(1, ScopeId(1001)),
        };
    }
    reject_action(body(1, vec![players]), WellFormedReason::UnsupportedShape);
}

fn search_condition(predicate: Expr<Bool>, destination_condition: bool) -> Located<Node> {
    let mut step = search();
    if let Action::Search {
        filter,
        destination,
        ..
    } = action(&mut step)
    {
        if destination_condition {
            *destination = Some(SearchDestination::OptionalSelfReplacement {
                decision: DecisionId(9002),
                predicate,
                movement: movement(shared(ZoneKind::Ex)),
            });
        } else {
            *filter = predicate;
        }
    }
    step
}
fn exists(objects: Selector) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::Exists { objects }))
}
fn looking(viewer: PlayerRef) -> Located<Node> {
    instruction(
        Action::Look {
            viewer: viewer.clone(),
            zone: ZoneRef {
                player: viewer,
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(1)),
            mode: StepwiseMode::Exact,
        },
        10,
        "synthetic",
    )
}
fn looked() -> Selector {
    Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look {
            receipt: ReceiptReference::new(ReceiptId(10)),
        },
        projection: MemberProjection::Before,
    }
}

#[test]
fn search_conditions_only_observe_the_searchers_private_information() {
    for destination_condition in [false, true] {
        for selector in [hand(), looked()] {
            accept(body(
                1,
                vec![
                    looking(PlayerRef::Controller {}),
                    search_condition(exists(selector), destination_condition),
                ],
            ));
        }
        let mut opponent_hand = hand();
        if let Selector::Zone { players, .. } = &mut opponent_hand {
            players.relation = PlayerRelation::Opponents;
        }
        for hidden in [
            opponent_hand,
            Selector::Ordered {
                zone: ZoneRef {
                    player: PlayerRef::Active {},
                    kind: ZoneKind::Deck,
                },
                start: Position::Top {},
                count: Expr::Literal(Nat(1)),
                direction: SliceDirection::Forward,
            },
            looked(),
        ] {
            let error = reject(
                body(
                    1,
                    vec![
                        looking(PlayerRef::Active {}),
                        search_condition(exists(hidden), destination_condition),
                    ],
                ),
                WellFormedReason::UnsupportedShape,
            );
            assert!(
                error.ir_path.ends_with(if destination_condition {
                    "/instruction/action/destination"
                } else {
                    "/instruction/action/filter"
                }),
                "{}",
                error.ir_path
            );
        }
    }
    // Engine instructions must not inherit the searcher's private-choice restrictions.
    for destination_condition in [false, true] {
        accept(body(
            1,
            vec![
                looking(PlayerRef::Active {}),
                search_condition(Expr::Literal(Bool(true)), destination_condition),
                reveal(20, looked()),
            ],
        ));
    }
}

#[test]
fn search_requires_a_destination_with_enough_partition_capacity() {
    let mut missing = search();
    if let Action::Search { destination, .. } = action(&mut missing) {
        *destination = None;
    }
    let missing_error = reject(body(1, vec![missing]), WellFormedReason::UnsupportedShape);
    assert!(missing_error.ir_path.ends_with("/action/destination"));
    for (maximum, parts, valid) in [
        (Some(3), vec![Some(1)], false),
        (None, vec![Some(3)], false),
        (Some(3), vec![Some(1), Some(2)], true),
        (None, vec![None], true),
        (Some(u64::MAX), vec![Some(u64::MAX), Some(1)], true),
    ] {
        let mut step = search();
        if let Action::Search {
            cardinality,
            destination,
            ..
        } = action(&mut step)
        {
            *cardinality = bounds(0, maximum);
            *destination = Some(SearchDestination::Partition {
                destinations: parts
                    .into_iter()
                    .enumerate()
                    .map(|(index, limit)| SearchPartition {
                        decision: DecisionId(
                            u64::try_from(index).unwrap().checked_add(9010).unwrap(),
                        ),
                        zone: MoveDestination::Owners {
                            zone: OwnerZone::Hand,
                        },
                        cardinality: bounds(0, limit),
                    })
                    .collect(),
            });
        }
        if valid {
            accept(body(1, vec![step]));
        } else {
            let error = reject(body(1, vec![step]), WellFormedReason::UnsupportedShape);
            assert!(error.ir_path.ends_with("/action/destination"));
        }
    }
}

#[test]
fn installed_cost_set_accepts_nonnegative_values_and_defined_selection() {
    for amount in [0, 3] {
        let mut step = install(1, objects());
        if let Action::InstallEffect { effect, .. } = action(&mut step)
            && let Effect::CostModifier { change, .. } = effect.as_mut()
        {
            *change = CostChange::Set {
                amount: Expr::Literal(Int(amount)),
            };
        }
        accept(body(1, vec![step]));
    }
    let choice = chosen_objects(10, 1, zone(PlayerRef::Controller {}, ZoneKind::Field, 20));
    accept(body(
        1,
        vec![
            decision(choice),
            install(
                1,
                Selector::Bound {
                    reference: Binding::new(10, ScopeId(1)),
                },
            ),
        ],
    ));
}
