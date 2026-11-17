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
                kind: CostKind::Play,
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
fn search_validates_every_destination_and_rejects_invalid_groups() {
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

mod ordering;
mod search_groups;
mod search_inputs;
