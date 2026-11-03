//! Only additional costs may reveal a fixed own-hand choice and move that same choice.
use super::{
    AbilitySource, Action, Binding, BodyContext, Instruction, InstructionContext, InstructionId,
    LoweringError, PlayerRef, ReceiptId, Selector, ast, unsupported,
};
use crate::ir::{
    ChoiceDomain, CostSpec, CostStep, Decision, DecisionId, DecisionKind, DiscardMode,
    ExecutionRef, InputMode, MoveDestination, MoveReason, MoveSpec, OwnerZone, PlaySlot, PlaySpec,
    Position, SelectionSnapshot, Visibility,
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
    let Action::Discard {
        mode: DiscardMode::Chosen { constraint },
        ..
    } = super::hand_payment::lower(context.card_allocator, material, path)?
    else {
        return Err(unsupported(path));
    };
    let [group] = constraint.groups.as_slice() else {
        return Err(unsupported(path));
    };
    let Selector::Zone { players, .. } = &group.selector else {
        return Err(unsupported(path));
    };
    let binding = Binding::new(context.allocator.id(path)?, context.scope);
    let decision = Decision {
        id: DecisionId(context.card_allocator.id(path)?),
        captures: vec![],
        actor: PlayerRef::Controller {},
        visibility: Visibility::Only {
            players: players.clone(),
        },
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
