use super::super::{
    CostSpec, CostStep, MoveDestination, MoveReason, MoveSpec, NumericChange, OwnerZone, PlaySpec,
    Posture, Resource, ResourceOperation,
};
use super::{
    AbilitySource, Action, BodyContext, Expr, Instruction, InstructionContext, InstructionId, Int,
    LoweringError, PlayerRef, ReceiptId, ast, numeric, unsupported,
};
use core::slice::from_ref;

pub(super) fn lower(
    context: &mut BodyContext<'_>,
    cost: &ast::Cost,
    source: &AbilitySource,
    path: &str,
) -> Result<PlaySpec, LoweringError> {
    let operations = match cost {
        ast::Cost::Short(operation) => from_ref(operation),
        ast::Cost::Ordered { steps } => steps.as_slice(),
    };
    let mut steps = Vec::new();
    let mut parameters = Vec::new();
    for operation in operations {
        let action = match operation {
            ast::CostOperation::ToCemetery(value)
            | ast::CostOperation::ReturnToHand(value)
            | ast::CostOperation::Banish(value)
            | ast::CostOperation::Act(value) => {
                let operation = match operation {
                    ast::CostOperation::ToCemetery(_) => super::material::Operation::Put,
                    ast::CostOperation::ReturnToHand(_) => super::material::Operation::Hand,
                    ast::CostOperation::Banish(_) => super::material::Operation::Banish,
                    ast::CostOperation::Act(_) => super::material::Operation::Act,
                    ast::CostOperation::Pp(_)
                    | ast::CostOperation::LeaderLife(_)
                    | ast::CostOperation::ActSelf(_)
                    | ast::CostOperation::SelfToCemetery(_) => return Err(unsupported(path)),
                };
                let (action, decision) = super::material::lower(
                    context.card_allocator,
                    &mut context.allocator,
                    context.scope,
                    value,
                    operation,
                    path,
                )?;
                parameters.push(decision);
                action
            }
            ast::CostOperation::Pp(amount) => Action::Resource {
                player: PlayerRef::Controller {},
                resource: Resource::PpCurrent,
                operation: ResourceOperation::Pay,
                amount: Expr::Literal(Int(
                    i64::try_from(*amount).map_err(|_private| unsupported(path))?
                )),
            },
            ast::CostOperation::LeaderLife(amount) => Action::Life {
                player: PlayerRef::Controller {},
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(i64::try_from(*amount)
                        .map_err(|_private| unsupported(path))?
                        .checked_neg()
                        .ok_or_else(|| unsupported(path))?)),
                },
            },
            ast::CostOperation::ActSelf(true) => Action::Posture {
                objects: numeric::holder(),
                posture: Posture::Acted,
            },
            ast::CostOperation::SelfToCemetery(true) => Action::Move {
                objects: numeric::holder(),
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
            ast::CostOperation::ActSelf(false) | ast::CostOperation::SelfToCemetery(false) => {
                return Err(unsupported(path));
            }
        };
        let id = context.card_allocator.id(path)?;
        steps.push(CostStep {
            materials: None,
            instruction: Box::new(Instruction {
                id: InstructionId(id),
                receipt: ReceiptId(id),
                actor: PlayerRef::Controller {},
                source: source.clone(),
                precondition: None,
                context: InstructionContext::Cost,
                action,
            }),
        });
    }
    if parameters.len() > 1 {
        return Err(unsupported(path));
    }
    Ok(PlaySpec {
        limit: None,
        captures: vec![],
        parameters,
        costs: CostSpec::Ordered { steps },
    })
}
