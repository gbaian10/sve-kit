use super::{
    Allocator, Binding, Cardinality, LoweringError, Nat, PlayerRef, PlayerRelation, PlayerSet,
    Selector, TextQuantifier, ZoneKind, ast, numeric, unsupported,
};
use crate::ir::{
    Action, ChoiceDomain, Decision, DecisionId, DecisionKind, Disjointness, ExecutionRef,
    InputMode, MoveDestination, MoveReason, MoveSpec, OwnerZone, PlaySlot, Posture, ScopeId,
    SelectionConstraint, SelectionGroup, SelectionSnapshot, SelectionView, Visibility, ZoneRef,
};

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Put,
    Hand,
    Banish,
    Act,
    Ex,
}

pub(super) fn lower(
    allocator: &mut Allocator,
    bindings: &mut Allocator,
    scope: ScopeId,
    material: &ast::Material,
    operation: Operation,
    path: &str,
) -> Result<(Action, Decision), LoweringError> {
    if material.n == 0 || !matches!(material.sel.who, ast::Side::Controller) {
        return Err(unsupported(path));
    }
    let zone = match material.sel.zone {
        ast::PublicZone::Field => ZoneKind::Field,
        ast::PublicZone::Ex => ZoneKind::Ex,
        ast::PublicZone::Cemetery if matches!(operation, Operation::Banish) => ZoneKind::Cemetery,
        ast::PublicZone::Cemetery => return Err(unsupported(path)),
    };
    if matches!(operation, Operation::Act | Operation::Ex) && zone != ZoneKind::Field {
        return Err(unsupported(path));
    }
    let binding = Binding::new(bindings.id(path)?, scope);
    let candidate = Binding::new(allocator.id(path)?, allocator.scope(path)?);
    let mut selector = Selector::Zone {
        players: PlayerSet {
            relation: PlayerRelation::SelfPlayer,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::Implicit,
        },
        zone,
        view: SelectionView::Current {},
        binding: candidate,
        predicate: Box::new(super::selection::field_filter(
            &super::selection::ObjectFilter::from(&material.sel),
            candidate,
        )),
    };
    if material.sel.other {
        selector = Selector::Difference {
            input: Box::new(selector),
            except: Box::new(numeric::holder()),
        };
    }
    let cardinality = Cardinality {
        minimum: Nat(material.n),
        maximum: Some(Nat(material.n)),
    };
    let objects = Selector::Bound { reference: binding };
    let action = material_action(objects, operation);
    Ok((
        action,
        Decision {
            id: DecisionId(allocator.id(path)?),
            captures: vec![],
            actor: PlayerRef::Controller {},
            domain: ChoiceDomain::Objects {
                constraint: SelectionConstraint {
                    groups: vec![SelectionGroup {
                        selector,
                        cardinality,
                    }],
                    cardinality,
                    distinct_by: None,
                    disjoint: Disjointness::Disjoint,
                    aggregate_bounds: vec![],
                },
                binding,
                target_roles: None,
            },
            kind: DecisionKind::PlayParameter {
                slot: PlaySlot::CostMaterials,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            },
            visibility: Visibility::Public {},
        },
    ))
}

const fn material_action(objects: Selector, operation: Operation) -> Action {
    match operation {
        Operation::Put | Operation::Hand => Action::Move {
            objects,
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: if matches!(operation, Operation::Hand) {
                        OwnerZone::Hand
                    } else {
                        OwnerZone::Cemetery
                    },
                },
                position: None,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        },
        Operation::Banish => Action::Banish { objects },
        Operation::Act => Action::Posture {
            objects,
            posture: Posture::Acted,
        },
        Operation::Ex => Action::Move {
            objects,
            movement: MoveSpec {
                to: MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex,
                    },
                },
                position: None,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        },
    }
}
