use super::{
    Allocator, Binding, Cardinality, LoweringError, Nat, PlayerRef, PlayerRelation, PlayerSet,
    Selector, TextQuantifier, ZoneKind, ast, numeric, unsupported,
};
use crate::ir::{
    Action, ChoiceDomain, Decision, DecisionId, DecisionKind, Disjointness, ExecutionRef,
    InputMode, MoveDestination, MoveReason, MoveSpec, OwnerZone, PlaySlot, Posture, ScopeId,
    SelectionConstraint, SelectionGroup, SelectionSnapshot, SelectionView, Visibility, ZoneRef,
};
use crate::ir::{AggregateBound, Comparison, Expr, Int, NumericProperty};

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
    if material.sel.acted.is_some() || material.distinct_by.is_some() {
        return Err(unsupported(path));
    }
    let cardinality = cardinality(material, path)?;
    let named = matches!(operation, Operation::Put)
        && matches!(material.sel.who, ast::Side::Controller)
        && matches!(material.sel.zone, ast::PublicZone::Field)
        && cardinality
            == (Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            });
    if !(material.sel.names.is_empty() || named) {
        return Err(unsupported(path));
    }
    if !(matches!(material.sel.who, ast::Side::Controller)
        || (matches!(material.sel.who, ast::Side::All)
            && matches!(material.sel.zone, ast::PublicZone::Field)
            && matches!(operation, Operation::Put | Operation::Banish)))
    {
        return Err(unsupported(path));
    }
    if (matches!(
        material.sel.zone,
        ast::PublicZone::Cemetery | ast::PublicZone::FieldEx
    ) && !matches!(operation, Operation::Banish)
        && !(matches!(operation, Operation::Put) && field_ex_token_material(material)))
        || (matches!(operation, Operation::Act | Operation::Ex)
            && !matches!(material.sel.zone, ast::PublicZone::Field))
    {
        return Err(unsupported(path));
    }
    let (objects, mut decision) =
        choice(allocator, bindings, scope, &material.sel, cardinality, path)?;
    if let Some(floor) = material.cost_sum_ge {
        // Only acting any number of own field followers has a card (BP11-P30) needing a floor.
        let (Operation::Act, ChoiceDomain::Objects { constraint, .. }) =
            (operation, &mut decision.domain)
        else {
            return Err(unsupported(path));
        };
        if !material.any || floor == 0 {
            return Err(unsupported(path));
        }
        constraint.aggregate_bounds.push(AggregateBound {
            property: NumericProperty::OriginalCost,
            comparison: Comparison::GreaterEqual,
            value: Expr::Literal(Int(
                i64::try_from(floor).map_err(|_overflow| unsupported(path))?
            )),
        });
    }
    Ok((material_action(objects, operation), decision))
}

// Only this token payment needs a cemetery choice across both public zones.
const fn field_ex_token_material(material: &ast::Material) -> bool {
    matches!(
        material,
        ast::Material {
            sel: ast::PublicQuery {
                acted: None,
                evolved: None,
                racing: None,
                other: false,
                not_same_name_as_this: false,
                who: ast::Side::Controller,
                zone: ast::PublicZone::FieldEx,
                kind: None,
                keyword: None,
                r#trait: Some(trait_name),
                traits_any: None,
                traits_all,
                name: None,
                names,
                name_contains: None,
                class: None,
                not_class: None,
                token: Some(true),
                orig_cost_eq: None,
                orig_cost_le: None,
                orig_cost_ge: None,
                cost_eq: None,
                cost_le: None,
                cost_ge: None,
                hp_le: None,
                r#where: None,
            },
            n: Some(1),
            any: false,
            result: None,
            distinct_by: None,
            cost_sum_ge: None,
        } if !trait_name.is_empty() && traits_all.is_empty() && names.is_empty()
    )
}

pub(super) fn cardinality(
    material: &ast::Material,
    path: &str,
) -> Result<Cardinality, LoweringError> {
    match (material.n, material.any) {
        (Some(count), false) if count > 0 => Ok(Cardinality {
            minimum: Nat(count),
            maximum: Some(Nat(count)),
        }),
        (None, true) => Ok(Cardinality {
            minimum: Nat(0),
            maximum: None,
        }),
        _ => Err(unsupported(path)),
    }
}

pub(super) fn choice(
    allocator: &mut Allocator,
    bindings: &mut Allocator,
    scope: ScopeId,
    query: &ast::PublicQuery,
    cardinality: Cardinality,
    path: &str,
) -> Result<(Selector, Decision), LoweringError> {
    if query.acted.is_some() || query.evolved.is_some() {
        return Err(unsupported(path));
    }
    if !matches!(query.who, ast::Side::Controller | ast::Side::All) {
        return Err(unsupported(path));
    }
    let zones = match query.zone {
        ast::PublicZone::Field => vec![ZoneKind::Field],
        ast::PublicZone::Ex => vec![ZoneKind::Ex],
        ast::PublicZone::Cemetery => vec![ZoneKind::Cemetery],
        ast::PublicZone::FieldEx
            if matches!(query.who, ast::Side::Controller) && cardinality.minimum.0 > 0 =>
        {
            vec![ZoneKind::Field, ZoneKind::Ex]
        }
        ast::PublicZone::FieldEx => return Err(unsupported(path)),
    };
    let binding = Binding::new(bindings.id(path)?, scope);
    let mut sets = Vec::new();
    for zone in zones {
        let candidate = Binding::new(allocator.id(path)?, allocator.scope(path)?);
        sets.push(Selector::Zone {
            players: PlayerSet {
                relation: if matches!(query.who, ast::Side::All) {
                    PlayerRelation::All
                } else {
                    PlayerRelation::SelfPlayer
                },
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(if matches!(query.who, ast::Side::All) {
                        2
                    } else {
                        1
                    })),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
            zone,
            view: SelectionView::Current {},
            binding: candidate,
            predicate: Box::new(super::selection::field_filter(
                &super::selection::ObjectFilter::from(query),
                candidate,
            )),
        });
    }
    let mut selector = if sets.len() == 1 {
        sets.remove(0)
    } else {
        Selector::Union { sets }
    };
    if query.other {
        selector = Selector::Difference {
            input: Box::new(selector),
            except: Box::new(numeric::holder()),
        };
    }
    let objects = Selector::Bound { reference: binding };
    Ok((
        objects,
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
