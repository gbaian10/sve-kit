use super::{BodyContext, LoweringError, ast, unsupported};
use crate::ir::{
    Binding, Cardinality, ChoiceDomain, Decision, DecisionId, DecisionKind, ExecutionRef, Expr,
    InputMode, Int, IntOperation, Nat, PlaySlot, PlayerRef, SelectionSnapshot, Visibility,
};
use core::slice::from_ref;

#[derive(Clone, Copy)]
pub(super) struct Parameter {
    pub(super) binding: Binding<Nat>,
    paid: bool,
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "X shares the ability's existing binding and parameter scope."
)]
impl BodyContext<'_> {
    pub(super) fn prepare_x(&mut self, body: &ast::Body, path: &str) -> Result<(), LoweringError> {
        let Some(declaration) = &body.x else {
            return Ok(());
        };
        if !declaration.is_empty() || self.definitions.contains_key("x") || !self.formal_play {
            return Err(unsupported(path));
        }
        self.declare_x(path)
    }

    pub(super) fn declare_x(&mut self, path: &str) -> Result<(), LoweringError> {
        let binding = Binding::new(self.allocator.id(path)?, self.scope);
        self.x = Some(Parameter {
            binding,
            paid: false,
        });
        self.parameters.push(Decision {
            id: DecisionId(self.card_allocator.id(path)?),
            actor: PlayerRef::Controller {},
            visibility: Visibility::Public {},
            captures: vec![],
            kind: DecisionKind::PlayParameter {
                slot: PlaySlot::X,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            },
            domain: ChoiceDomain::Integer {
                binding,
                cardinality: Cardinality {
                    minimum: Nat(0),
                    maximum: None,
                },
            },
        });
        Ok(())
    }

    pub(super) fn check_x(&self, path: &str) -> Result<(), LoweringError> {
        if self.x.is_some_and(|parameter| !parameter.paid) {
            return Err(unsupported(path));
        }
        Ok(())
    }

    pub(super) fn counter_amount(
        &mut self,
        value: Option<&ast::PpAmount>,
        any: bool,
        path: &str,
    ) -> Result<Expr<Nat>, LoweringError> {
        match (value, any) {
            (Some(ast::PpAmount::Literal(value)), false) => {
                i64::try_from(*value).map_err(|_overflow| unsupported(path))?;
                Ok(Expr::Literal(Nat(*value)))
            }
            (Some(ast::PpAmount::Variable(ast::PpVariable::X)), false) => {
                let parameter = self.x.as_mut().ok_or_else(|| unsupported(path))?;
                parameter.paid = true;
                Ok(Expr::Bound(parameter.binding))
            }
            (None, true) if self.formal_play => {
                let binding = Binding::new(self.allocator.id(path)?, self.scope);
                self.parameters.push(Decision {
                    id: DecisionId(self.card_allocator.id(path)?),
                    actor: PlayerRef::Controller {},
                    visibility: Visibility::Public {},
                    captures: vec![],
                    kind: DecisionKind::PlayParameter {
                        slot: PlaySlot::X,
                        selection_snapshot: SelectionSnapshot::AtDecision,
                        play_scope: ExecutionRef::Current {},
                        input: InputMode::Inline,
                    },
                    domain: ChoiceDomain::Integer {
                        binding,
                        cardinality: Cardinality {
                            minimum: Nat(0),
                            maximum: None,
                        },
                    },
                });
                Ok(Expr::Bound(binding))
            }
            _ => Err(unsupported(path)),
        }
    }

    pub(super) fn pp_amount(
        &mut self,
        value: &ast::PpAmount,
        path: &str,
    ) -> Result<Expr<Int>, LoweringError> {
        match value {
            ast::PpAmount::Literal(value) => Ok(Expr::Literal(Int(
                i64::try_from(*value).map_err(|_private| unsupported(path))?
            ))),
            ast::PpAmount::Variable(ast::PpVariable::X) => {
                let parameter = self.x.as_mut().ok_or_else(|| unsupported(path))?;
                parameter.paid = true;
                let binding = parameter.binding;
                Ok(Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Bound(binding),
                })))
            }
        }
    }
}

pub(super) fn uses_pp_x(cost: &ast::Cost) -> bool {
    let steps = match cost {
        ast::Cost::Short(step) => from_ref(step.as_ref()),
        ast::Cost::Ordered { steps } => steps.as_slice(),
    };
    steps.iter().any(|step| {
        matches!(
            step,
            ast::CostOperation::Pp(ast::PpAmount::Variable(ast::PpVariable::X))
        )
    })
}
