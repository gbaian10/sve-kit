use super::{Allocator, Located, LoweringError, SourceSpan, ast, costs, unsupported};
use crate::ir::{
    Action, Binding, Bool, Cardinality, ChoiceDomain, CostChange, CostSpec, CostStep, Decision,
    DecisionId, DecisionKind, DiscardMode, Disjointness, ExecutionRef, Expr, InputMode,
    Instruction, InstructionContext, InstructionId, Int, MoveDestination, MoveReason, MoveSpec,
    Nat, OwnerZone, PlayCostExchange, PlaySlot, PlayerRef, Posture, ReceiptId, SelectionConstraint,
    SelectionGroup, SelectionSnapshot, SelectionView, Selector, Visibility, ZoneKind,
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
    let binding = Binding::new(allocator.id(&path)?, scope);
    let (materials, action) = lower_material(optional, binding, allocator, &path)?;
    let id = allocator.id(&path)?;
    let mut located = source.clone();
    located.author_path.clone_from(&path);
    Ok(Some(Located {
        source: located,
        node: PlayCostExchange {
            scope,
            decision: DecisionId(allocator.id(&path)?),
            materials,
            cost: CostSpec::Ordered {
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
            },
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
        (None, Some(material), None) if matches!(material.sel.zone, ast::PublicZone::Cemetery) => {
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
    let materials = materials(material, binding, allocator, path)?;
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
) -> Result<Decision, LoweringError> {
    let cardinality = Cardinality {
        minimum: Nat(material.n.ok_or_else(|| unsupported(path))?),
        maximum: Some(Nat(material.n.ok_or_else(|| unsupported(path))?)),
    };
    costs::query(&material.sel, allocator, path)?;
    let selector = costs::public_query(
        &material.sel,
        Binding::new(allocator.id(path)?, allocator.scope(path)?),
        path,
    )?;
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
