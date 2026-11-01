use super::{Action, Expr, LoweringError, Nat, ObjectOperand, ast, unsupported};
use crate::ir::{CounterKind, CounterOperation, Holder};

pub(super) const fn holder() -> Holder {
    Holder::Object {
        object: ObjectOperand::SourceHolder {},
    }
}

pub(super) fn change(
    value: &ast::CounterChange,
    holder: Holder,
    path: &str,
) -> Result<Action, LoweringError> {
    let amount = value.delta.checked_abs().ok_or_else(|| unsupported(path))?;
    Ok(Action::Counter {
        holder,
        kind: CounterKind(value.name.clone()),
        operation: if value.delta < 0 {
            CounterOperation::Remove {}
        } else {
            CounterOperation::Add {}
        },
        amount: Expr::Literal(Nat(
            u64::try_from(amount).map_err(|_private| unsupported(path))?
        )),
    })
}

pub(super) fn cost(value: &ast::CounterCost, path: &str) -> Result<Action, LoweringError> {
    let ast::This::This = value.on;
    i64::try_from(value.n).map_err(|_private| unsupported(path))?;
    Ok(Action::Counter {
        holder: holder(),
        kind: CounterKind(value.name.clone()),
        operation: CounterOperation::Remove {},
        amount: Expr::Literal(Nat(value.n)),
    })
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Counter lowering shares the ability binding environment."
)]
impl super::BodyContext<'_> {
    pub(super) fn counter_step(
        &mut self,
        value: &ast::CounterChange,
        path: &str,
        source: &super::SourceSpan,
        origin: &super::AbilitySource,
    ) -> Result<super::Located<super::Node>, LoweringError> {
        use crate::ir::{
            Binding, Control, Instruction, InstructionContext, InstructionId, Located, Node,
            PlayerRef, Program, ReceiptId,
        };
        let input = self.objects(&value.on, path)?;
        let scope = self.card_allocator.scope(path)?;
        let binding = Binding::new(self.allocator.id(path)?, scope);
        let id = self.card_allocator.id(path)?;
        let mut location = source.clone();
        path.clone_into(&mut location.author_path);
        Ok(Located {
            source: location.clone(),
            node: Node::Control {
                control: Control::EachObject {
                    input,
                    binding,
                    order: None,
                    body: Program {
                        scope,
                        steps: vec![Located {
                            source: location,
                            node: Node::Instruction {
                                instruction: Box::new(Instruction {
                                    id: InstructionId(id),
                                    receipt: ReceiptId(id),
                                    actor: PlayerRef::Controller {},
                                    source: origin.clone(),
                                    precondition: None,
                                    context: InstructionContext::Effect,
                                    action: change(
                                        value,
                                        Holder::Object {
                                            object: ObjectOperand::Bound { binding },
                                        },
                                        path,
                                    )?,
                                }),
                            },
                        }],
                    },
                },
            },
        })
    }
}
