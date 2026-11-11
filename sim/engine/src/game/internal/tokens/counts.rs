//! Count admission composes only public-zone and successful Before readers.

use super::super::{Execution, Game, Result, invalid, receipt_members, unsupported, values};
use super::dynamic;
use crate::ir::{
    Arithmetic, Bool, Comparison, DestroyOutcome, Expr, Int, IntOperation, Nat, NatOperation,
    NatRead, PlayerRef, Predicate, ReceiptReference,
};
use core::convert::identity;

#[derive(Clone, Debug)]
pub(super) struct CountPlan(Natural);

#[derive(Clone, Debug)]
enum Natural {
    Literal(u64),
    PublicZone(Box<Expr<Nat>>),
    DestroyedController(ReceiptReference<DestroyOutcome>, PlayerRef),
    CheckedInt(Box<Integer>),
    Conditional(Box<Boolean>, Box<Self>, Box<Self>),
}

#[derive(Clone, Debug)]
enum Integer {
    Literal(i64),
    FromNat(Box<Natural>),
    Arithmetic(Arithmetic, Vec<Self>),
    Conditional(Box<Boolean>, Box<Self>, Box<Self>),
}

#[derive(Clone, Debug)]
enum Boolean {
    Literal(bool),
    And(Vec<Self>),
    Or(Vec<Self>),
    Not(Box<Self>),
    CompareInt(Box<Integer>, Comparison, Box<Integer>),
    CompareNat(Box<Natural>, Comparison, Box<Natural>),
}

#[derive(Clone, Copy)]
struct Bounds {
    lower: i64,
    upper: i64,
}

impl Bounds {
    const fn point(value: i64) -> Self {
        Self {
            lower: value,
            upper: value,
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            lower: self.lower.min(other.lower),
            upper: self.upper.max(other.upper),
        }
    }

    fn binary(self, operation: Arithmetic, right: Self) -> Result<Self> {
        Ok(match operation {
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
                return Err(unsupported("token arithmetic proof operands"));
            }
        })
    }
}

struct CheckedNatural {
    plan: Natural,
    constant: Option<u64>,
}

struct CheckedInteger {
    plan: Integer,
    bounds: Bounds,
    constant: Option<i64>,
}

struct CheckedBoolean {
    plan: Boolean,
    constant: Option<bool>,
}

impl CountPlan {
    pub(super) fn compile(value: &Expr<Nat>) -> Result<Self> {
        natural(value).map(|checked| Self(checked.plan))
    }

    pub(super) fn evaluate(&self, execution: &Execution, game: &Game) -> Result<u64> {
        self.0.evaluate(execution, game)
    }

    pub(super) fn matches_player(&self, expected: &PlayerRef) -> bool {
        let mut matches = true;
        self.0
            .visit_before("", &mut |player, _| matches &= player == expected);
        matches
    }

    pub(super) fn before_paths(&self, prefix: &str) -> Vec<String> {
        let mut paths = Vec::new();
        self.0
            .visit_before(prefix, &mut |_, path| paths.push(path.into()));
        paths
    }

    pub(super) fn mask_before(value: &mut Expr<Nat>) {
        if Self::compile(value).is_ok() {
            mask_natural(value);
        }
    }
}

fn natural(expression: &Expr<Nat>) -> Result<CheckedNatural> {
    let (plan, constant) = match expression {
        Expr::Literal(Nat(count)) => (Natural::Literal(*count), Some(*count)),
        Expr::Live(read)
            if matches!(read.as_ref(), NatRead::ZoneCount { selector }
            if super::super::card::selection::field_selector(selector).is_ok()
                || super::super::card::selection::public_zone_selector(selector).is_ok()) =>
        {
            (Natural::PublicZone(Box::new(expression.clone())), None)
        }
        Expr::Compute(operation) => match operation.as_ref() {
            NatOperation::CheckedInt { value } => {
                let checked = integer(value)?;
                if checked.bounds.lower < 0 {
                    return Err(unsupported("token count cannot be proved nonnegative"));
                }
                let constant = checked
                    .constant
                    .map(u64::try_from)
                    .transpose()
                    .map_err(invalid)?;
                (Natural::CheckedInt(Box::new(checked.plan)), constant)
            }
            NatOperation::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                let condition = boolean(condition)?;
                let yes = natural(then_value)?;
                let no = natural(else_value)?;
                let constant = condition
                    .constant
                    .and_then(|selected| if selected { yes.constant } else { no.constant });
                (
                    Natural::Conditional(
                        Box::new(condition.plan),
                        Box::new(yes.plan),
                        Box::new(no.plan),
                    ),
                    constant,
                )
            }
            NatOperation::Count { .. } => {
                let (receipt, player) = dynamic::projection(expression)
                    .ok_or_else(|| unsupported("token count receipt reader"))?;
                (
                    Natural::DestroyedController(receipt.clone(), player.clone()),
                    None,
                )
            }
            _ => return Err(unsupported("token natural count operation")),
        },
        _ => return Err(unsupported("token natural count reader")),
    };
    Ok(CheckedNatural { plan, constant })
}

fn integer(expression: &Expr<Int>) -> Result<CheckedInteger> {
    match expression {
        Expr::Literal(Int(value)) => Ok(CheckedInteger {
            plan: Integer::Literal(*value),
            bounds: Bounds::point(*value),
            constant: Some(*value),
        }),
        Expr::Compute(operation) => match operation.as_ref() {
            IntOperation::FromNat { value } => {
                let checked = natural(value)?;
                let constant =
                    checked
                        .constant
                        .map(i64::try_from)
                        .transpose()
                        .map_err(|error| {
                            unsupported(&format!(
                                "constant token count exceeds signed domain: {error}"
                            ))
                        })?;
                let bounds = constant.map_or(
                    Bounds {
                        lower: 0,
                        upper: i64::MAX,
                    },
                    Bounds::point,
                );
                Ok(CheckedInteger {
                    plan: Integer::FromNat(Box::new(checked.plan)),
                    bounds,
                    constant,
                })
            }
            IntOperation::Arithmetic {
                operation,
                arguments,
            } => arithmetic(*operation, arguments),
            IntOperation::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                let condition = boolean(condition)?;
                let yes = integer(then_value)?;
                let no = integer(else_value)?;
                let constant = condition
                    .constant
                    .and_then(|selected| if selected { yes.constant } else { no.constant });
                Ok(CheckedInteger {
                    plan: Integer::Conditional(
                        Box::new(condition.plan),
                        Box::new(yes.plan),
                        Box::new(no.plan),
                    ),
                    bounds: yes.bounds.union(no.bounds),
                    constant,
                })
            }
            _ => Err(unsupported("token integer count operation")),
        },
        _ => Err(unsupported("token integer count reader")),
    }
}

fn arithmetic(operation: Arithmetic, arguments: &[Expr<Int>]) -> Result<CheckedInteger> {
    let expected = match operation {
        Arithmetic::HalfUp => 1..=1,
        Arithmetic::Minimum | Arithmetic::Maximum => 1..=usize::MAX,
        _ => 2..=2,
    };
    if !expected.contains(&arguments.len()) {
        return Err(unsupported("token arithmetic arity"));
    }
    if operation == Arithmetic::FloorDivide
        && !matches!(arguments.last(), Some(Expr::Literal(Int(divisor))) if *divisor > 0)
    {
        return Err(unsupported("positive literal token divisor"));
    }
    let checked = arguments.iter().map(integer).collect::<Result<Vec<_>>>()?;
    let constant = checked
        .iter()
        .map(|value| value.constant)
        .collect::<Option<Vec<_>>>()
        .map(|values| {
            values::arithmetic(operation, &values).map_err(|error| {
                unsupported(&format!("constant token arithmetic overflow: {error}"))
            })
        })
        .transpose()?;
    let first = checked
        .first()
        .ok_or_else(|| unsupported("empty token arithmetic proof"))?
        .bounds;
    let bounds = if operation == Arithmetic::HalfUp {
        Bounds {
            lower: values::arithmetic(operation, &[first.lower])?,
            upper: values::arithmetic(operation, &[first.upper])?,
        }
    } else {
        checked
            .iter()
            .skip(1)
            .try_fold(first, |left, right| left.binary(operation, right.bounds))?
    };
    Ok(CheckedInteger {
        plan: Integer::Arithmetic(
            operation,
            checked.into_iter().map(|value| value.plan).collect(),
        ),
        bounds,
        constant,
    })
}

fn boolean(expression: &Expr<Bool>) -> Result<CheckedBoolean> {
    let (plan, constant) = match expression {
        Expr::Literal(Bool(value)) => (Boolean::Literal(*value), Some(*value)),
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                let checked = predicates.iter().map(boolean).collect::<Result<Vec<_>>>()?;
                let constants = checked
                    .iter()
                    .map(|value| value.constant)
                    .collect::<Option<Vec<_>>>();
                let plans = checked.into_iter().map(|value| value.plan).collect();
                if matches!(operation.as_ref(), Predicate::And { .. }) {
                    (
                        Boolean::And(plans),
                        constants.map(|values| values.into_iter().all(identity)),
                    )
                } else {
                    (
                        Boolean::Or(plans),
                        constants.map(|values| values.into_iter().any(identity)),
                    )
                }
            }
            Predicate::Not { predicate } => {
                let checked = boolean(predicate)?;
                (
                    Boolean::Not(Box::new(checked.plan)),
                    checked.constant.map(|value| !value),
                )
            }
            Predicate::CompareInt {
                left,
                comparison,
                right,
            } => {
                let left = integer(left)?;
                let right = integer(right)?;
                let constant = left
                    .constant
                    .zip(right.constant)
                    .map(|(left, right)| values::compare(left, *comparison, right));
                (
                    Boolean::CompareInt(Box::new(left.plan), *comparison, Box::new(right.plan)),
                    constant,
                )
            }
            Predicate::CompareNat {
                left,
                comparison,
                right,
            } => {
                let left = natural(left)?;
                let right = natural(right)?;
                let constant = left
                    .constant
                    .zip(right.constant)
                    .map(|(left, right)| values::compare(left, *comparison, right));
                (
                    Boolean::CompareNat(Box::new(left.plan), *comparison, Box::new(right.plan)),
                    constant,
                )
            }
            _ => return Err(unsupported("token count predicate")),
        },
        _ => return Err(unsupported("token count boolean reader")),
    };
    Ok(CheckedBoolean { plan, constant })
}

impl Natural {
    fn evaluate(&self, execution: &Execution, game: &Game) -> Result<u64> {
        match self {
            Self::Literal(value) => Ok(*value),
            Self::PublicZone(value) => execution.natural(game, value).map(|value| value.0),
            Self::DestroyedController(receipt, player) => {
                receipt_members::destroyed_controller(execution, game, receipt, player)
            }
            Self::CheckedInt(value) => {
                u64::try_from(value.evaluate(execution, game)?).map_err(invalid)
            }
            Self::Conditional(condition, yes, no) => {
                let selected = if condition.evaluate(execution, game)? {
                    yes
                } else {
                    no
                };
                selected.evaluate(execution, game)
            }
        }
    }

    fn visit_before(&self, path: &str, visitor: &mut impl FnMut(&PlayerRef, &str)) {
        match self {
            Self::DestroyedController(_, player) => visitor(player, path),
            Self::CheckedInt(value) => {
                value.visit_before(&format!("{path}/compute/value"), visitor);
            }
            Self::Conditional(condition, yes, no) => {
                condition.visit_before(&format!("{path}/compute/condition"), visitor);
                yes.visit_before(&format!("{path}/compute/then_value"), visitor);
                no.visit_before(&format!("{path}/compute/else_value"), visitor);
            }
            Self::Literal(_) | Self::PublicZone(_) => {}
        }
    }
}

impl Integer {
    fn evaluate(&self, execution: &Execution, game: &Game) -> Result<i64> {
        match self {
            Self::Literal(value) => Ok(*value),
            Self::FromNat(value) => {
                i64::try_from(value.evaluate(execution, game)?).map_err(invalid)
            }
            Self::Arithmetic(operation, arguments) => {
                let values = arguments
                    .iter()
                    .map(|value| value.evaluate(execution, game))
                    .collect::<Result<Vec<_>>>()?;
                values::arithmetic(*operation, &values)
            }
            Self::Conditional(condition, yes, no) => {
                let selected = if condition.evaluate(execution, game)? {
                    yes
                } else {
                    no
                };
                selected.evaluate(execution, game)
            }
        }
    }

    fn visit_before(&self, path: &str, visitor: &mut impl FnMut(&PlayerRef, &str)) {
        match self {
            Self::FromNat(value) => value.visit_before(&format!("{path}/compute/value"), visitor),
            Self::Arithmetic(_, arguments) => {
                for (index, argument) in arguments.iter().enumerate() {
                    argument.visit_before(&format!("{path}/compute/arguments/{index}"), visitor);
                }
            }
            Self::Conditional(condition, yes, no) => {
                condition.visit_before(&format!("{path}/compute/condition"), visitor);
                yes.visit_before(&format!("{path}/compute/then_value"), visitor);
                no.visit_before(&format!("{path}/compute/else_value"), visitor);
            }
            Self::Literal(_) => {}
        }
    }
}

impl Boolean {
    fn visit_before(&self, path: &str, visitor: &mut impl FnMut(&PlayerRef, &str)) {
        match self {
            Self::And(predicates) | Self::Or(predicates) => {
                for (index, predicate) in predicates.iter().enumerate() {
                    predicate.visit_before(&format!("{path}/compute/predicates/{index}"), visitor);
                }
            }
            Self::Not(predicate) => {
                predicate.visit_before(&format!("{path}/compute/predicate"), visitor);
            }
            Self::CompareInt(left, _, right) => {
                left.visit_before(&format!("{path}/compute/left"), visitor);
                right.visit_before(&format!("{path}/compute/right"), visitor);
            }
            Self::CompareNat(left, _, right) => {
                left.visit_before(&format!("{path}/compute/left"), visitor);
                right.visit_before(&format!("{path}/compute/right"), visitor);
            }
            Self::Literal(_) => {}
        }
    }

    fn evaluate(&self, execution: &Execution, game: &Game) -> Result<bool> {
        match self {
            Self::Literal(value) => Ok(*value),
            Self::And(predicates) => Ok(predicates
                .iter()
                .map(|value| value.evaluate(execution, game))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .all(identity)),
            Self::Or(predicates) => Ok(predicates
                .iter()
                .map(|value| value.evaluate(execution, game))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .any(identity)),
            Self::Not(value) => value.evaluate(execution, game).map(|value| !value),
            Self::CompareInt(left, comparison, right) => Ok(values::compare(
                left.evaluate(execution, game)?,
                *comparison,
                right.evaluate(execution, game)?,
            )),
            Self::CompareNat(left, comparison, right) => Ok(values::compare(
                left.evaluate(execution, game)?,
                *comparison,
                right.evaluate(execution, game)?,
            )),
        }
    }
}

fn mask_natural(expression: &mut Expr<Nat>) {
    if dynamic::projection(expression).is_some() {
        *expression = Expr::Literal(Nat(0));
        return;
    }
    if let Expr::Compute(operation) = expression {
        match operation.as_mut() {
            NatOperation::CheckedInt { value } => mask_integer(value),
            NatOperation::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                mask_boolean(condition);
                mask_natural(then_value);
                mask_natural(else_value);
            }
            _ => {}
        }
    }
}

fn mask_integer(expression: &mut Expr<Int>) {
    if let Expr::Compute(operation) = expression {
        match operation.as_mut() {
            IntOperation::FromNat { value } => mask_natural(value),
            IntOperation::Arithmetic { arguments, .. } => {
                arguments.iter_mut().for_each(mask_integer);
            }
            IntOperation::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                mask_boolean(condition);
                mask_integer(then_value);
                mask_integer(else_value);
            }
            _ => {}
        }
    }
}

fn mask_boolean(value: &mut Expr<Bool>) {
    if let Expr::Compute(predicate) = value {
        match predicate.as_mut() {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                predicates.iter_mut().for_each(mask_boolean);
            }
            Predicate::Not { predicate } => mask_boolean(predicate),
            Predicate::CompareInt { left, right, .. } => {
                mask_integer(left);
                mask_integer(right);
            }
            Predicate::CompareNat { left, right, .. } => {
                mask_natural(left);
                mask_natural(right);
            }
            _ => {}
        }
    }
}
