use super::{Action, BodyContext, Expr, LoweringError, Nat, ObjectOperand, ast, unsupported};
use crate::ir::{CounterKind, CounterOperation, Decision, Holder, Selector};

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

pub(super) fn cost(
    context: &mut BodyContext<'_>,
    value: &ast::CounterCost,
    path: &str,
) -> Result<(Action, Option<Decision>), LoweringError> {
    use crate::ir::ChoiceDomain;
    i64::try_from(value.n).map_err(|_private| unsupported(path))?;
    let (holder, parameter) = match &value.on {
        ast::CounterCostHolder::This(_) if value.holders.is_none() => (holder(), None),
        ast::CounterCostHolder::Selected(query)
            if value.holders == Some(1)
                && matches!(query.who, ast::Side::Controller)
                && matches!(query.zone, ast::PublicZone::Field | ast::PublicZone::Ex) =>
        {
            let (_, mut decision) = super::material::choice(
                context.card_allocator,
                &mut context.allocator,
                context.scope,
                query,
                1,
                path,
            )?;
            let ChoiceDomain::Objects {
                binding,
                constraint,
                ..
            } = &mut decision.domain
            else {
                return Err(unsupported(path));
            };
            let selected = *binding;
            for group in &mut constraint.groups {
                require_counter(&mut group.selector, &value.name, value.n, path)?;
            }
            (
                Holder::Object {
                    object: ObjectOperand::Selected {
                        selection: selected,
                    },
                },
                Some(decision),
            )
        }
        ast::CounterCostHolder::This(_) | ast::CounterCostHolder::Selected(_) => {
            return Err(unsupported(path));
        }
    };
    Ok((
        Action::Counter {
            holder,
            kind: CounterKind(value.name.clone()),
            operation: CounterOperation::Remove {},
            amount: Expr::Literal(Nat(value.n)),
        },
        parameter,
    ))
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only the material builder's Zone and Difference shapes receive a counter guard."
)]
fn require_counter(
    selector: &mut Selector,
    name: &str,
    amount: u64,
    path: &str,
) -> Result<(), LoweringError> {
    use crate::ir::{Comparison, Int, IntOperation, NatRead, Predicate, Selector};
    match selector {
        Selector::Zone {
            binding, predicate, ..
        } => {
            let enough = Expr::Compute(Box::new(Predicate::CompareInt {
                left: Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Live(Box::new(NatRead::Counter {
                        holder: Holder::Object {
                            object: ObjectOperand::Bound { binding: *binding },
                        },
                        kind: CounterKind(name.to_owned()),
                    })),
                })),
                comparison: Comparison::GreaterEqual,
                right: Expr::Literal(Int(
                    i64::try_from(amount).map_err(|_private| unsupported(path))?
                )),
            }));
            **predicate = Expr::Compute(Box::new(Predicate::And {
                predicates: vec![(**predicate).clone(), enough],
            }));
            Ok(())
        }
        Selector::Difference { input, .. } => require_counter(input, name, amount, path),
        _ => Err(unsupported(path)),
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Counter lowering shares the ability binding environment."
)]
impl BodyContext<'_> {
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
