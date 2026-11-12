//! Conservative bounds describe successful checked results, never runtime clamping.

use super::{Arithmetic, Expr, Int, IntOperation};

#[derive(Clone, Copy)]
pub(crate) struct Bounds {
    pub(crate) lower: i64,
    pub(crate) upper: i64,
}

impl Bounds {
    pub(crate) const fn point(value: i64) -> Self {
        Self {
            lower: value,
            upper: value,
        }
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self {
            lower: self.lower.min(other.lower),
            upper: self.upper.max(other.upper),
        }
    }

    pub(crate) fn binary(self, operation: Arithmetic, right: Self) -> Option<Self> {
        Some(match operation {
            // Saturation bounds successful checked results; it never changes runtime values.
            Arithmetic::Add => Self {
                lower: self.lower.saturating_add(right.lower),
                upper: self.upper.saturating_add(right.upper),
            },
            Arithmetic::Subtract => Self {
                lower: self.lower.saturating_sub(right.upper),
                upper: self.upper.saturating_sub(right.lower),
            },
            Arithmetic::Multiply => {
                let a = self.lower.saturating_mul(right.lower);
                let b = self.lower.saturating_mul(right.upper);
                let c = self.upper.saturating_mul(right.lower);
                let d = self.upper.saturating_mul(right.upper);
                Self {
                    lower: a.min(b).min(c).min(d),
                    upper: a.max(b).max(c).max(d),
                }
            }
            Arithmetic::Minimum => Self {
                lower: self.lower.min(right.lower),
                upper: self.upper.min(right.upper),
            },
            Arithmetic::Maximum => Self {
                lower: self.lower.max(right.lower),
                upper: self.upper.max(right.upper),
            },
            Arithmetic::FloorDivide if right.lower == right.upper && right.lower > 0 => Self {
                lower: self.lower.div_euclid(right.lower),
                upper: self.upper.div_euclid(right.lower),
            },
            Arithmetic::FloorDivide | Arithmetic::HalfUp => {
                return None;
            }
        })
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Opaque signed readers have no structural sign proof."
)]
pub(crate) fn integer(expression: &Expr<Int>) -> Bounds {
    let unknown = Bounds {
        lower: i64::MIN,
        upper: i64::MAX,
    };
    match expression {
        Expr::Literal(Int(value)) => Bounds::point(*value),
        Expr::Compute(operation) => match operation.as_ref() {
            IntOperation::FromNat { value } => {
                if let Expr::Literal(value) = value
                    && let Ok(value) = i64::try_from(value.0)
                {
                    return Bounds::point(value);
                }
                Bounds {
                    lower: 0,
                    upper: i64::MAX,
                }
            }
            IntOperation::Arithmetic {
                operation,
                arguments,
            } => {
                let valid = match operation {
                    Arithmetic::Minimum | Arithmetic::Maximum => !arguments.is_empty(),
                    Arithmetic::HalfUp => false,
                    Arithmetic::Add
                    | Arithmetic::Subtract
                    | Arithmetic::Multiply
                    | Arithmetic::FloorDivide => arguments.len() == 2,
                };
                if !valid {
                    return unknown;
                }
                let mut values = arguments.iter().map(integer);
                let Some(first) = values.next() else {
                    return unknown;
                };
                values
                    .try_fold(first, |left, right| left.binary(*operation, right))
                    .unwrap_or(unknown)
            }
            IntOperation::Conditional {
                then_value,
                else_value,
                ..
            } => integer(then_value).union(integer(else_value)),
            _ => unknown,
        },
        Expr::Frozen(_) | Expr::Live(_) | Expr::Captured(_) | Expr::Bound(_) => unknown,
    }
}
