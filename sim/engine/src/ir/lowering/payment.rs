use super::super::{CostSpec, CostStep, PlaySpec, Posture, Resource, ResourceOperation};
use super::{
    AbilitySource, Action, Allocator, Expr, Instruction, InstructionContext, InstructionId, Int,
    LoweringError, PlayerRef, ReceiptId, ast, numeric, unsupported,
};
use core::slice::from_ref;

pub(super) fn lower(
    allocator: &mut Allocator,
    cost: &ast::Cost,
    source: &AbilitySource,
    path: &str,
) -> Result<PlaySpec, LoweringError> {
    let operations = match cost {
        ast::Cost::Short(operation) => from_ref(operation),
        ast::Cost::Ordered { steps } => steps.as_slice(),
    };
    let mut steps = Vec::new();
    for operation in operations {
        let action = match operation {
            ast::CostOperation::Pp(amount) => Action::Resource {
                player: PlayerRef::Controller {},
                resource: Resource::PpCurrent,
                operation: ResourceOperation::Pay,
                amount: Expr::Literal(Int(
                    i64::try_from(*amount).map_err(|_private| unsupported(path))?
                )),
            },
            ast::CostOperation::ActSelf(true) => Action::Posture {
                objects: numeric::holder(),
                posture: Posture::Acted,
            },
            ast::CostOperation::ActSelf(false) => return Err(unsupported(path)),
        };
        let id = allocator.id(path)?;
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
    Ok(PlaySpec {
        captures: vec![],
        parameters: vec![],
        costs: CostSpec::Ordered { steps },
    })
}
