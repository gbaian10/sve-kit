use super::{Allocator, Located, LoweringError, SourceSpan, ast, costs, unsupported};
use crate::ir::{
    Action, Binding, Bool, Cardinality, ChoiceDomain, CostChange, CostSpec, CostStep, Decision,
    DecisionId, DecisionKind, DiscardMode, Disjointness, ExecutionRef, Expr, InputMode,
    Instruction, InstructionContext, InstructionId, Int, MoveDestination, MoveReason, MoveSpec,
    Nat, ObjectOperand, ObjectView, OwnerZone, PlayCostExchange, PlaySlot, PlayerRef, Posture,
    PostureRead, Predicate, ReceiptId, SelectionConstraint, SelectionGroup, SelectionSnapshot,
    SelectionView, Selector, Visibility, ZoneKind,
};

enum MaterialAction {
    Put,
    Banish,
    Act,
}

pub(super) fn lower(
    play: &ast::PlayCosts,
    source: &SourceSpan,
    allocator: &mut Allocator,
) -> Result<Option<Located<PlayCostExchange>>, LoweringError> {
    let path = format!("{}/play/optional_cost", source.author_path);
    let Some(optional) = &play.optional_cost else {
        if play.optional_cost_delta.is_some() || play.optional_cost_set.is_some() {
            return Err(unsupported(&path));
        }
        return Ok(None);
    };
    let change = match (play.optional_cost_delta, play.optional_cost_set) {
        (Some(value), None) if value <= 0 => CostChange::Delta {
            amount: Expr::Literal(Int(value)),
        },
        (None, Some(value)) if value >= 0 => CostChange::Set {
            amount: Expr::Literal(Int(value)),
        },
        _ => return Err(unsupported(&path)),
    };
    let scope = allocator.scope(&path)?;
    let choices = if let Some(routes) = &optional.one_of {
        if routes.len() != 2
            || optional.discard.is_some()
            || optional.to_cemetery.is_some()
            || optional.banish.is_some()
            || optional.act.is_some()
            || optional.result.is_some()
            || routes.iter().any(|route| {
                route.one_of.is_some() || route.discard.is_some() || route.result.is_some()
            })
        {
            return Err(unsupported(&path));
        }
        routes.iter().collect::<Vec<_>>()
    } else {
        vec![optional]
    };
    let multiple = choices.len() == 2;
    let mut materials = Vec::new();
    let mut alternatives = Vec::new();
    for (index, choice) in choices.into_iter().enumerate() {
        let route_path = format!("{path}/one_of/{index}");
        let binding = Binding::new(allocator.id(&route_path)?, scope);
        let (decision, action) = lower_material(choice, binding, allocator, &route_path, multiple)?;
        materials.push(decision);
        let id = allocator.id(&route_path)?;
        alternatives.push(CostSpec::Ordered {
            steps: vec![CostStep {
                keyword: None,
                materials: None,
                instruction: Box::new(Instruction {
                    id: InstructionId(id),
                    receipt: ReceiptId(id),
                    actor: PlayerRef::Controller {},
                    source: super::ability_source(source, None),
                    precondition: None,
                    context: InstructionContext::Cost,
                    action,
                }),
            }],
        });
    }
    let cost = if multiple {
        CostSpec::Alternative { alternatives }
    } else {
        alternatives.pop().ok_or_else(|| unsupported(&path))?
    };
    let mut located = source.clone();
    located.author_path.clone_from(&path);
    Ok(Some(Located {
        source: located,
        node: PlayCostExchange {
            scope,
            decision: DecisionId(allocator.id(&path)?),
            materials,
            route: if multiple {
                Some(DecisionId(allocator.id(&path)?))
            } else {
                None
            },
            cost,
            change,
            receipt: ReceiptId(allocator.id(&path)?),
        },
    }))
}

fn lower_material(
    optional: &ast::OptionalCost,
    binding: Binding<Selector>,
    allocator: &mut Allocator,
    path: &str,
    multiple: bool,
) -> Result<(Decision, Action), LoweringError> {
    if let Some(discard) = &optional.discard {
        if optional.to_cemetery.is_some() || optional.banish.is_some() || optional.act.is_some() {
            return Err(unsupported(path));
        }
        let Action::Discard {
            mode: DiscardMode::Chosen { constraint },
            ..
        } = super::hand_payment::lower(allocator, discard, path)?
        else {
            return Err(unsupported(path));
        };
        if constraint.cardinality.minimum.0 == 0
            || constraint.cardinality.maximum != Some(constraint.cardinality.minimum)
        {
            return Err(unsupported(path));
        }
        let [group] = constraint.groups.as_slice() else {
            return Err(unsupported(path));
        };
        let Selector::Zone { players, .. } = &group.selector else {
            return Err(unsupported(path));
        };
        let visibility = Visibility::Only {
            players: players.clone(),
        };
        let action = Action::Discard {
            player: PlayerRef::Controller {},
            mode: DiscardMode::All {
                eligible: Selector::Intersection {
                    sets: vec![
                        Selector::Zone {
                            players: players.clone(),
                            zone: ZoneKind::Hand,
                            view: SelectionView::Current {},
                            binding: Binding::new(allocator.id(path)?, allocator.scope(path)?),
                            predicate: Box::new(Expr::Literal(Bool(true))),
                        },
                        Selector::Bound { reference: binding },
                    ],
                },
            },
        };
        return Ok((
            decision(binding, constraint, visibility, allocator, path)?,
            action,
        ));
    }
    let (material, operation) = match (&optional.to_cemetery, &optional.banish, &optional.act) {
        (Some(material), None, None) if matches!(material.sel.zone, ast::PublicZone::Field) => {
            (material, MaterialAction::Put)
        }
        (None, Some(material), None)
            if matches!(material.sel.zone, ast::PublicZone::Cemetery)
                || (multiple && matches!(material.sel.zone, ast::PublicZone::Ex)) =>
        {
            (material, MaterialAction::Banish)
        }
        (None, None, Some(material)) if matches!(material.sel.zone, ast::PublicZone::Field) => {
            (material, MaterialAction::Act)
        }
        _ => return Err(unsupported(path)),
    };
    if material.n.is_none_or(|n| n == 0)
        || material.any
        || material.result.is_some()
        || !matches!(material.sel.who, ast::Side::Controller)
    {
        return Err(unsupported(path));
    }
    let materials = materials(material, binding, allocator, path, multiple)?;
    let objects = Selector::Bound { reference: binding };
    let action = match operation {
        MaterialAction::Put => Action::Move {
            objects,
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Cemetery,
                },
                position: None,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        },
        MaterialAction::Banish => Action::Banish { objects },
        MaterialAction::Act => Action::Posture {
            objects,
            posture: Posture::Acted,
        },
    };
    Ok((materials, action))
}

fn materials(
    material: &ast::Material,
    binding: Binding<Selector>,
    allocator: &mut Allocator,
    path: &str,
    multiple: bool,
) -> Result<Decision, LoweringError> {
    let cardinality = Cardinality {
        minimum: Nat(material.n.ok_or_else(|| unsupported(path))?),
        maximum: Some(Nat(material.n.ok_or_else(|| unsupported(path))?)),
    };
    let mut query = material.sel.clone();
    let standing = if multiple {
        if query.class.as_ref().is_none_or(String::is_empty)
            || !matches!(
                (&query.zone, query.acted),
                (ast::PublicZone::Field, Some(false)) | (ast::PublicZone::Ex, None)
            )
        {
            return Err(unsupported(path));
        }
        let standing = query.acted.take() == Some(false);
        let class = query.class.take();
        costs::query(&query, allocator, path)?;
        query.class = class;
        standing
    } else {
        costs::query(&query, allocator, path)?;
        false
    };
    let candidate = Binding::new(allocator.id(path)?, allocator.scope(path)?);
    let mut selector = costs::public_query(&query, candidate, path)?;
    if standing {
        let Selector::Zone { predicate, .. } = &mut selector else {
            return Err(unsupported(path));
        };
        **predicate = Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::PostureEquals {
                    left: Expr::Live(Box::new(PostureRead::Posture {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::Bound { binding: candidate }),
                        },
                    })),
                    right: Expr::Literal(Posture::Standing),
                })),
                *predicate.clone(),
            ],
        }));
    }
    decision(
        binding,
        SelectionConstraint {
            groups: vec![SelectionGroup {
                selector,
                cardinality,
            }],
            cardinality,
            distinct_by: None,
            disjoint: Disjointness::Disjoint,
            aggregate_bounds: vec![],
        },
        Visibility::Public {},
        allocator,
        path,
    )
}

fn decision(
    binding: Binding<Selector>,
    constraint: SelectionConstraint,
    visibility: Visibility,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Decision, LoweringError> {
    Ok(Decision {
        id: DecisionId(allocator.id(path)?),
        captures: vec![],
        actor: PlayerRef::Controller {},
        domain: ChoiceDomain::Objects {
            constraint,
            binding,
            target_roles: None,
        },
        kind: DecisionKind::PlayParameter {
            slot: PlaySlot::CostMaterials,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        visibility,
    })
}

pub(super) fn bindings(
    cost: &ast::OptionalCost,
    exchange: &PlayCostExchange,
    path: &str,
) -> Result<super::PaymentBindings, LoweringError> {
    let complete = cost.result.clone().map(|name| (name, exchange.receipt));
    let discard = if let Some(material) = &cost.discard {
        let CostSpec::Ordered { steps } = &exchange.cost else {
            return Err(unsupported(path));
        };
        let receipt = steps
            .first()
            .ok_or_else(|| unsupported(path))?
            .instruction
            .receipt;
        let bounds = super::hand_payment::cardinality(material, path)?;
        material.result.clone().map(|name| (name, receipt, bounds))
    } else {
        None
    };
    if complete
        .as_ref()
        .zip(discard.as_ref())
        .is_some_and(|((name, _), (other, ..))| name == other)
    {
        return Err(unsupported(path));
    }
    Ok(super::PaymentBindings { complete, discard })
}
