//! Numeric declarations are scoped values, separate from payment parameters and dice.
use super::{Action, Binding, BodyContext, Expr, Int, LoweringError, Nat, ast, unsupported};
use crate::ir::{Declaration, IntOperation};

#[derive(Clone, Copy)]
pub(super) enum DeclaredBinding {
    Natural(Binding<Nat>),
    Integer(Binding<Int>),
}
impl DeclaredBinding {
    pub(super) fn integer(self) -> Expr<Int> {
        match self {
            Self::Natural(binding) => Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Bound(binding),
            })),
            Self::Integer(binding) => Expr::Bound(binding),
        }
    }
}

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Declarations use the body's lexical binding environment."
)]
impl BodyContext<'_> {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only declared naturals and literal nonnegative values bypass signed arithmetic."
    )]
    pub(super) fn declared_natural(&self, value: &ast::Number) -> Option<Expr<Nat>> {
        match value {
            ast::Number::Alias(name) => match self.declared_numbers.get(name)? {
                DeclaredBinding::Natural(binding) => Some(Expr::Bound(*binding)),
                DeclaredBinding::Integer(_) => None,
            },
            ast::Number::Literal(number) => u64::try_from(*number)
                .ok()
                .map(|number| Expr::Literal(Nat(number))),
            _ => None,
        }
    }
    pub(super) fn declare(
        &mut self,
        value: &ast::Declare,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let ast::DeclareNumber::Number = value.kind;
        if self.declared_numbers.contains_key(&value.bind)
            || self.definitions.contains_key(&value.bind)
            || self.dice_results.contains_key(&value.bind)
            || value.bind == "x"
        {
            return Err(unsupported(path));
        }
        let actor = self.participant(value.who.as_ref(), path)?;
        let id = self.allocator.id(path)?;
        let (declaration, bound) = match (value.min, value.max) {
            (None | Some(0), None) => {
                let binding = Binding::new(id, self.scope);
                (
                    Declaration::NonnegativeInteger { binding },
                    DeclaredBinding::Natural(binding),
                )
            }
            (minimum, Some(maximum)) if minimum.unwrap_or(0) <= maximum => {
                let binding = Binding::new(id, self.scope);
                (
                    Declaration::BoundedInteger {
                        minimum: Int(i64::try_from(minimum.unwrap_or(0))
                            .map_err(|_private| unsupported(path))?),
                        maximum: Int(i64::try_from(maximum).map_err(|_private| unsupported(path))?),
                        binding,
                    },
                    DeclaredBinding::Integer(binding),
                )
            }
            _ => return Err(unsupported(path)),
        };
        self.declared_numbers.insert(value.bind.clone(), bound);
        Ok(Action::Declare { actor, declaration })
    }
}
