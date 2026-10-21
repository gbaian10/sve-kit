use super::*;

fn grouped() -> Located<Node> {
    let mut step = search();
    if let Action::Search {
        groups,
        cardinality,
        ..
    } = action(&mut step)
    {
        let mut constraint = constraint(zone(PlayerRef::Controller {}, ZoneKind::Deck, 10));
        constraint.cardinality = *cardinality;
        constraint.groups[0].cardinality = *cardinality;
        constraint.distinct_by = Some(ObjectProperty::Names);
        constraint.aggregate_bounds.push(AggregateBound {
            property: NumericProperty::Cost,
            comparison: Comparison::LessEqual,
            value: Expr::Literal(Int(7)),
        });
        *groups = Some(constraint);
    }
    step
}

fn continued(revealed: bool) -> Located<Node> {
    let mut step = search();
    if let Action::Search {
        destination,
        continuation,
        ..
    } = action(&mut step)
    {
        let objects = if revealed {
            Selector::SearchRevealed {
                revealed: continuation.revealed,
            }
        } else {
            Selector::SearchFound {
                found: continuation.found,
            }
        };
        *destination = Some(SearchDestination::OptionalSelfReplacement {
            decision: DecisionId(9002),
            predicate: Expr::Compute(Box::new(Predicate::Exists { objects })),
            movement: movement(shared(ZoneKind::Field)),
        });
    }
    step
}

#[test]
fn cp04_filter_seeds_grouped_search_without_a_second_selection_or_deck_escape() {
    // The grouping is synthetic; CP04-060 seeds only the global search filter and destination.
    accept(body(1, vec![grouped()]));
    for change in 0_u8..7 {
        let mut step = grouped();
        if let Action::Search {
            groups: Some(groups),
            ..
        } = action(&mut step)
        {
            match change {
                0 => groups.cardinality = bounds(0, Some(2)),
                1 => groups.disjoint = Disjointness::MayOverlap,
                2 => groups.groups.clear(),
                3 => groups.groups[0].selector = zone(PlayerRef::Owner {}, ZoneKind::Deck, 10),
                4 => groups.groups[0].selector = zone(PlayerRef::Controller {}, ZoneKind::Hand, 10),
                5 => groups.groups[0].cardinality = bounds(2, Some(1)),
                _ => groups.aggregate_bounds[0].value = Expr::Bound(Binding::new(99, ScopeId(1))),
            }
        }
        let (reason, suffix) = match change {
            0..=2 => (WellFormedReason::UnsupportedShape, "/groups"),
            3 | 4 => (WellFormedReason::UnsupportedShape, "/groups/0"),
            5 => (WellFormedReason::InvalidCardinality, "/groups/0"),
            _ => (
                WellFormedReason::UnavailableBinding,
                "/groups/aggregate_bounds/0",
            ),
        };
        let error = reject(body(1, vec![step]), reason);
        assert!(error.ir_path.ends_with(suffix), "{error}");
    }
    let mut leak = grouped();
    if let Action::Search {
        groups: Some(groups),
        ..
    } = action(&mut leak)
        && let Selector::Zone { predicate, .. } = &mut groups.groups[0].selector
    {
        **predicate = Expr::Compute(Box::new(Predicate::Exists {
            objects: zone(PlayerRef::Controller {}, ZoneKind::Deck, 11),
        }));
    }
    reject(body(1, vec![leak]), WellFormedReason::UnsupportedShape);
}

#[test]
fn search_continuation_projects_found_and_revealed_but_never_moved_after() {
    for revealed in [false, true] {
        accept(body(1, vec![continued(revealed)]));
    }
    for objects in [
        Selector::SearchFound {
            found: Binding::new(2, ScopeId(3)),
        },
        Selector::SearchRevealed {
            revealed: Binding::new(3, ScopeId(3)),
        },
    ] {
        reject(
            body(1, vec![search(), reveal(2, objects)]),
            WellFormedReason::UnavailableBinding,
        );
    }
    let mut wrong = continued(true);
    if let Action::Search {
        destination: Some(SearchDestination::OptionalSelfReplacement { predicate, .. }),
        ..
    } = action(&mut wrong)
    {
        *predicate = Expr::Compute(Box::new(Predicate::Exists {
            objects: Selector::SearchRevealed {
                revealed: Binding::new(2, ScopeId(3)),
            },
        }));
    }
    reject(body(1, vec![wrong]), WellFormedReason::BindingType);
    let mut before_produced = continued(true);
    if let Action::Search {
        destination: Some(SearchDestination::OptionalSelfReplacement { predicate, .. }),
        ..
    } = action(&mut before_produced)
    {
        *predicate = Expr::Compute(Box::new(Predicate::Exists {
            objects: after(MovementReceipt::Search {
                receipt: ReceiptReference::new(ReceiptId(1)),
            }),
        }));
    }
    let error = reject(
        body(1, vec![before_produced]),
        WellFormedReason::UnsupportedShape,
    );
    assert!(error.ir_path.ends_with("/destination"), "{error}");
}

#[test]
fn search_rejects_destination_order_and_position_until_shuffle_sequencing_is_defined() {
    for replacement in [false, true] {
        for (with_order, positioned) in [(false, true), (true, false), (true, true)] {
            let mut step = continued(true);
            let mut movement = movement(MoveDestination::Owners {
                zone: OwnerZone::Deck,
            });
            movement.position = positioned.then_some(Position::Bottom {});
            if with_order {
                movement.order = Some(ordering::order_decision(
                    Selector::SearchRevealed {
                        revealed: Binding::new(3, ScopeId(3)),
                    },
                    Visibility::Public {},
                ));
            }
            if let Action::Search { destination, .. } = action(&mut step) {
                *destination = Some(if replacement {
                    SearchDestination::OptionalSelfReplacement {
                        decision: DecisionId(9002),
                        predicate: Expr::Literal(Bool(true)),
                        movement,
                    }
                } else {
                    SearchDestination::Single { movement }
                });
            }
            let error = reject(body(1, vec![step]), WellFormedReason::UnsupportedShape);
            assert!(error.ir_path.ends_with("/destination"), "{error}");
        }
    }
}

#[test]
fn search_group_aggregate_cannot_read_the_raw_deck() {
    let mut step = grouped();
    if let Action::Search {
        groups: Some(groups),
        ..
    } = action(&mut step)
    {
        groups.aggregate_bounds[0].value = Expr::Compute(Box::new(IntOperation::Sum {
            objects: zone(PlayerRef::Controller {}, ZoneKind::Deck, 11),
            property: NumericProperty::Cost,
        }));
    }
    let error = reject(body(1, vec![step]), WellFormedReason::UnsupportedShape);
    assert!(
        error.ir_path.ends_with("/groups/aggregate_bounds/0"),
        "{error}"
    );
}
