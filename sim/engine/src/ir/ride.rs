//! The closed Ride projection is shared by lowering and runtime admission.
use super::{
    Action, Binding, BoolRead, BooleanState, CardName, Cardinality, ChoiceDomain, Decision,
    DecisionId, DecisionKind, Disjointness, ExecutionRef, Expr, InputMode, Nat, ObjectOperand,
    ObjectView, PlaySlot, PlayerRef, PlayerRelation, PlayerSet, Predicate, RideDriveOperation,
    ScopeId, SelectionConstraint, SelectionGroup, SelectionSnapshot, SelectionView, Selector,
    TextQuantifier, UsageReference, Visibility, ZoneKind,
};

pub(crate) const fn gain_drive() -> Action {
    Action::RideDrive {
        subject: ObjectOperand::SourceHolder {},
        operation: RideDriveOperation::GainDrive {},
        usage: UsageReference::CurrentRulePermission,
    }
}

pub(crate) const fn attachment(scope: ScopeId) -> Action {
    Action::RideDrive {
        subject: ObjectOperand::SourceHolder {},
        operation: RideDriveOperation::AttachDrivePoint {
            resource: ObjectOperand::Selected {
                selection: Binding::new(0, scope),
            },
        },
        usage: UsageReference::CurrentRulePermission,
    }
}

pub(crate) fn parameter(scope: ScopeId) -> Decision {
    let one = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    let candidate = Binding::new(1, scope);
    Decision {
        id: DecisionId(scope.0),
        captures: vec![],
        actor: PlayerRef::Controller {},
        visibility: Visibility::Only {
            players: PlayerSet {
                relation: PlayerRelation::SelfPlayer,
                relative_to: PlayerRef::Controller {},
                cardinality: one,
                text_quantifier: TextQuantifier::Implicit,
            },
        },
        kind: DecisionKind::PlayParameter {
            slot: PlaySlot::CostMaterials,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        domain: ChoiceDomain::Objects {
            binding: Binding::new(0, scope),
            target_roles: None,
            constraint: SelectionConstraint {
                groups: vec![SelectionGroup {
                    cardinality: one,
                    selector: Selector::Zone {
                        players: PlayerSet {
                            relation: PlayerRelation::SelfPlayer,
                            relative_to: PlayerRef::Controller {},
                            cardinality: one,
                            text_quantifier: TextQuantifier::Implicit,
                        },
                        zone: ZoneKind::Evolve,
                        view: SelectionView::Current {},
                        binding: candidate,
                        predicate: Box::new(Expr::Compute(Box::new(Predicate::And {
                            predicates: vec![
                                Expr::Compute(Box::new(Predicate::NameEquals {
                                    object: ObjectView::Current {
                                        object: Box::new(ObjectOperand::Bound {
                                            binding: candidate,
                                        }),
                                    },
                                    name: Expr::Literal(CardName("ドライブポイント".into())),
                                })),
                                Expr::Compute(Box::new(Predicate::Not {
                                    predicate: Expr::Live(Box::new(BoolRead::State {
                                        object: ObjectView::Current {
                                            object: Box::new(ObjectOperand::Bound {
                                                binding: candidate,
                                            }),
                                        },
                                        property: BooleanState::FaceUp,
                                    })),
                                })),
                            ],
                        }))),
                    },
                }],
                cardinality: one,
                disjoint: Disjointness::Disjoint,
                distinct_by: None,
                aggregate_bounds: vec![],
            },
        },
    }
}
