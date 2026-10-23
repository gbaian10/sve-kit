use super::{Allocator, Located, LoweringError, SourceSpan, ast, costs, unsupported};
use crate::ir::{
    Action, Binding, Cardinality, ChoiceDomain, CostChange, CostSpec, CostStep, Decision,
    DecisionId, DecisionKind, Disjointness, ExecutionRef, Expr, InputMode, Instruction,
    InstructionContext, InstructionId, Int, MoveDestination, MoveReason, MoveSpec, Nat, OwnerZone,
    PlayCostExchange, PlaySlot, PlayerRef, Posture, ReceiptId, SelectionConstraint, SelectionGroup,
    SelectionSnapshot, Selector, Visibility,
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
        _ => return Err(unsupported(&path)),
    };
    if material.n == 0 || !matches!(material.sel.who, ast::Side::Controller) {
        return Err(unsupported(&path));
    }
    let scope = allocator.scope(&path)?;
    let binding = Binding::new(allocator.id(&path)?, scope);
    let materials = materials(material, binding, allocator, &path)?;
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

fn materials(
    material: &ast::Material,
    binding: Binding<Selector>,
    allocator: &mut Allocator,
    path: &str,
) -> Result<Decision, LoweringError> {
    let cardinality = Cardinality {
        minimum: Nat(material.n),
        maximum: Some(Nat(material.n)),
    };
    let mut selector = costs::query(&material.sel, allocator, path)?;
    if let Selector::Zone {
        binding: candidate,
        predicate,
        ..
    } = &mut selector
    {
        *candidate = Binding::new(allocator.id(path)?, allocator.scope(path)?);
        **predicate = super::search::object_filter(
            material.sel.kind.as_ref(),
            material.sel.r#trait.as_deref(),
            material.sel.name.as_deref(),
            *candidate,
        );
    }
    let decision = Decision {
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
    };
    Ok(decision)
}
