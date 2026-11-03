//! Revealing materials share one choice; only additional costs may also move that choice.
use super::{
    AbilitySource, Action, Binding, BodyContext, Instruction, InstructionContext, InstructionId,
    LoweringError, PlayerRef, ReceiptId, Selector, ast, unsupported,
};
use crate::ir::{
    ChoiceDomain, CostSpec, CostStep, Decision, DecisionId, DecisionKind, ExecutionRef, InputMode,
    MoveDestination, MoveReason, MoveSpec, OwnerZone, PlaySlot, PlaySpec, Position,
    SelectionSnapshot, Visibility,
};

pub(super) fn lower(
    context: &mut BodyContext<'_>,
    operations: &[ast::CostOperation],
    source: &AbilitySource,
    path: &str,
) -> Result<PlaySpec, LoweringError> {
    let [ast::CostOperation::Reveal(material), rest @ ..] = operations else {
        return Err(unsupported(path));
    };
    let count = super::hand_payment::cardinality(material, path)?;
    if !matches!(count.minimum.0, 1 | 2) || count.maximum != Some(count.minimum) {
        return Err(unsupported(path));
    }
    let move_bottom = match rest {
        [] if material.result.is_none() => false,
        [ast::CostOperation::ToDeckBottom(next)]
            if next.n == 1
                && count.minimum.0 == 1
                && material
                    .result
                    .as_ref()
                    .is_some_and(|name| !name.is_empty() && name == &next.sel.r#in) =>
        {
            true
        }
        _ => return Err(unsupported(path)),
    };
    if material.sel.name_contains.is_some() {
        return Err(unsupported(path));
    }
    let decision = choice(context, material, path)?;
    let ChoiceDomain::Objects { binding, .. } = decision.domain.clone() else {
        return Err(unsupported(path));
    };
    let selected = Selector::Bound { reference: binding };
    let mut actions = vec![Action::Reveal {
        objects: selected.clone(),
    }];
    if move_bottom {
        actions.push(Action::Move {
            objects: selected,
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                },
                position: Some(Position::Bottom {}),
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        });
    }
    let mut steps = vec![];
    for action in actions {
        let id = context.card_allocator.id(path)?;
        steps.push(CostStep {
            keyword: None,
            materials: None,
            instruction: Box::new(Instruction {
                id: InstructionId(id),
                receipt: ReceiptId(id),
                source: source.clone(),
                actor: PlayerRef::Controller {},
                context: InstructionContext::Cost,
                precondition: None,
                action,
            }),
        });
    }
    Ok(PlaySpec {
        limit: None,
        captures: vec![],
        parameters: vec![decision],
        costs: CostSpec::Ordered { steps },
    })
}

fn choice(
    context: &mut BodyContext<'_>,
    material: &ast::HandMaterial,
    path: &str,
) -> Result<Decision, LoweringError> {
    let count = super::hand_payment::cardinality(material, path)?;
    if !matches!(count.minimum.0, 1 | 2) || count.maximum != Some(count.minimum) {
        return Err(unsupported(path));
    }
    let constraint = super::hand_payment::constraint(context.card_allocator, material, path)?;
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported(path));
    };
    let Selector::Zone { players, .. } = &group.selector else {
        return Err(unsupported(path));
    };
    let players = players.clone();
    let binding = Binding::new(context.allocator.id(path)?, context.scope);
    Ok(Decision {
        id: DecisionId(context.card_allocator.id(path)?),
        captures: vec![],
        actor: PlayerRef::Controller {},
        visibility: Visibility::Only { players },
        kind: DecisionKind::PlayParameter {
            slot: PlaySlot::CostMaterials,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        domain: ChoiceDomain::Objects {
            constraint,
            binding,
            target_roles: None,
        },
    })
}

pub(super) fn field(
    context: &mut BodyContext<'_>,
    material: &ast::HandMaterial,
    path: &str,
) -> Result<(Action, Decision), LoweringError> {
    if material.n.is_none() || material.upto.is_some() || material.any || material.result.is_some()
    {
        return Err(unsupported(path));
    }
    let decision = choice(context, material, path)?;
    let ChoiceDomain::Objects { binding, .. } = &decision.domain else {
        return Err(unsupported(path));
    };
    Ok((
        Action::Reveal {
            objects: Selector::Bound {
                reference: *binding,
            },
        },
        decision,
    ))
}
