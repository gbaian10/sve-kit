//! Shared numeric composition keeps count proofs separate from effect evaluation.

use super::tokens::dynamic;
use super::{Execution, Game, Result, invalid, receipt_members, unsupported, values};
use crate::ir::{
    Arithmetic, Binding, Bool, Comparison, DestroyOutcome, Expr, Int, IntOperation, Nat,
    NatOperation, NatRead, PlayerRef, Predicate, ReceiptReference, number_bounds::Bounds,
};
use core::convert::identity;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Policy {
    TokenCount(Option<Binding<Nat>>),
    EffectAmount,
    CounterAmount,
}

impl Policy {
    const fn opaque_leaves(self) -> bool {
        matches!(self, Self::EffectAmount | Self::CounterAmount)
    }
}

#[derive(Clone, Debug)]
pub(super) struct CountPlan(Natural);

#[derive(Clone, Debug)]
pub(super) struct NaturalAmountPlan(Natural);

impl NaturalAmountPlan {
    pub(super) fn compile(value: &Expr<Nat>) -> Result<Self> {
        if matches!(value, Expr::Literal(Nat(count)) if i64::try_from(*count).is_err()) {
            return Err(unsupported("counter amount exceeds signed representation"));
        }
        natural(value, Policy::CounterAmount).map(|checked| Self(checked.plan))
    }

    pub(super) fn evaluate(&self, execution: &Execution, game: &Game) -> Result<u64> {
        self.0.evaluate(execution, game)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AmountPlan {
    constant: Option<i64>,
    lower: i64,
}

impl AmountPlan {
    pub(super) fn compile(value: &Expr<Int>) -> Result<Self> {
        let checked = integer(value, Policy::EffectAmount)?;
        Ok(Self {
            constant: checked.constant,
            lower: checked.bounds.lower,
        })
    }

    pub(super) fn nonnegative_constant(&self) -> Option<i64> {
        self.constant.filter(|_| self.lower >= 0)
    }
}

#[derive(Clone, Debug)]
enum Natural {
    Literal(u64),
    PaidX(Binding<Nat>),
    PublicZone(Box<Expr<Nat>>),
    Effect(Box<Expr<Nat>>),
    DestroyedController(ReceiptReference<DestroyOutcome>, PlayerRef),
    CheckedInt(Box<Integer>),
    Conditional(Box<Boolean>, Box<Self>, Box<Self>),
}

#[derive(Clone, Debug)]
enum Integer {
    Literal(i64),
    Effect(Box<Expr<Int>>),
    FromNat(Box<Natural>),
    Arithmetic(Arithmetic, Vec<Self>),
    Conditional(Box<Boolean>, Box<Self>, Box<Self>),
}

#[derive(Clone, Debug)]
enum Boolean {
    Literal(bool),
    Effect(Box<Expr<Bool>>),
    And(Vec<Self>),
    Or(Vec<Self>),
    Not(Box<Self>),
    CompareInt(Box<Integer>, Comparison, Box<Integer>),
    CompareNat(Box<Natural>, Comparison, Box<Natural>),
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
        Self::compile_with_x(value, None)
    }

    pub(super) fn compile_with_x(value: &Expr<Nat>, x: Option<Binding<Nat>>) -> Result<Self> {
        natural(value, Policy::TokenCount(x)).map(|checked| Self(checked.plan))
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

    pub(super) fn mask_before(value: &mut Expr<Nat>, x: Option<Binding<Nat>>) {
        if Self::compile_with_x(value, x).is_ok() {
            mask_natural(value);
        }
    }
}

fn natural(expression: &Expr<Nat>, policy: Policy) -> Result<CheckedNatural> {
    let (plan, constant) = match expression {
        Expr::Literal(Nat(count)) => (Natural::Literal(*count), Some(*count)),
        Expr::Bound(binding) if matches!(policy, Policy::TokenCount(Some(expected)) if expected == *binding) => {
            (Natural::PaidX(*binding), None)
        }
        Expr::Live(read)
            if matches!(read.as_ref(), NatRead::ZoneCount { selector }
            if super::card::selection::field_selector(selector).is_ok()
                || super::card::selection::public_zone_selector(selector).is_ok()) =>
        {
            (Natural::PublicZone(Box::new(expression.clone())), None)
        }
        Expr::Compute(operation) => match operation.as_ref() {
            NatOperation::CheckedInt { value } => {
                let checked = integer(value, policy)?;
                if policy != Policy::EffectAmount && checked.bounds.lower < 0 {
                    return Err(unsupported(if matches!(policy, Policy::TokenCount(_)) {
                        "token count cannot be proved nonnegative"
                    } else {
                        "counter amount cannot be proved nonnegative"
                    }));
                }
                let constant = if matches!(policy, Policy::TokenCount(_)) {
                    checked
                        .constant
                        .map(u64::try_from)
                        .transpose()
                        .map_err(invalid)?
                } else {
                    checked
                        .constant
                        .and_then(|constant| u64::try_from(constant).ok())
                };
                (Natural::CheckedInt(Box::new(checked.plan)), constant)
            }
            NatOperation::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                let condition = boolean(condition, policy)?;
                let yes = natural(then_value, policy)?;
                let no = natural(else_value, policy)?;
                let constant = match policy {
                    Policy::TokenCount(_) => condition
                        .constant
                        .and_then(|selected| if selected { yes.constant } else { no.constant }),
                    Policy::EffectAmount | Policy::CounterAmount => condition
                        .constant
                        .zip(yes.constant)
                        .zip(no.constant)
                        .map(|((selected, yes), no)| if selected { yes } else { no }),
                };
                (
                    Natural::Conditional(
                        Box::new(condition.plan),
                        Box::new(yes.plan),
                        Box::new(no.plan),
                    ),
                    constant,
                )
            }
            NatOperation::Count { .. } if matches!(policy, Policy::TokenCount(_)) => {
                let (receipt, player) = dynamic::projection(expression)
                    .ok_or_else(|| unsupported("token count receipt reader"))?;
                (
                    Natural::DestroyedController(receipt.clone(), player.clone()),
                    None,
                )
            }
            _ if policy.opaque_leaves() => (Natural::Effect(Box::new(expression.clone())), None),
            _ => return Err(unsupported("token natural count operation")),
        },
        _ if policy.opaque_leaves() => (Natural::Effect(Box::new(expression.clone())), None),
        _ => return Err(unsupported("token natural count reader")),
    };
    Ok(CheckedNatural { plan, constant })
}

fn integer(expression: &Expr<Int>, policy: Policy) -> Result<CheckedInteger> {
    match expression {
        Expr::Literal(Int(value)) => Ok(CheckedInteger {
            plan: Integer::Literal(*value),
            bounds: Bounds::point(*value),
            constant: Some(*value),
        }),
        Expr::Compute(operation) => match operation.as_ref() {
            IntOperation::FromNat { value } => {
                let checked = natural(value, policy)?;
                let constant = if matches!(policy, Policy::TokenCount(_)) {
                    checked
                        .constant
                        .map(i64::try_from)
                        .transpose()
                        .map_err(|error| {
                            unsupported(&format!(
                                "constant token count exceeds signed domain: {error}"
                            ))
                        })?
                } else {
                    checked
                        .constant
                        .and_then(|constant| i64::try_from(constant).ok())
                };
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
            } => arithmetic(*operation, arguments, policy),
            IntOperation::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                let condition = boolean(condition, policy)?;
                let yes = integer(then_value, policy)?;
                let no = integer(else_value, policy)?;
                let constant = match policy {
                    Policy::TokenCount(_) => condition
                        .constant
                        .and_then(|selected| if selected { yes.constant } else { no.constant }),
                    Policy::EffectAmount | Policy::CounterAmount => condition
                        .constant
                        .zip(yes.constant)
                        .zip(no.constant)
                        .map(|((selected, yes), no)| if selected { yes } else { no }),
                };
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
            _ if policy.opaque_leaves() => Ok(CheckedInteger {
                plan: Integer::Effect(Box::new(expression.clone())),
                bounds: Bounds {
                    lower: i64::MIN,
                    upper: i64::MAX,
                },
                constant: None,
            }),
            _ => Err(unsupported("token integer count operation")),
        },
        _ if policy.opaque_leaves() => Ok(CheckedInteger {
            plan: Integer::Effect(Box::new(expression.clone())),
            bounds: Bounds {
                lower: i64::MIN,
                upper: i64::MAX,
            },
            constant: None,
        }),
        _ => Err(unsupported("token integer count reader")),
    }
}

fn arithmetic(
    operation: Arithmetic,
    arguments: &[Expr<Int>],
    policy: Policy,
) -> Result<CheckedInteger> {
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
    let checked = arguments
        .iter()
        .map(|value| integer(value, policy))
        .collect::<Result<Vec<_>>>()?;
    let folded = checked
        .iter()
        .map(|value| value.constant)
        .collect::<Option<Vec<_>>>()
        .map(|values| values::arithmetic(operation, &values));
    let constant = match (policy, folded) {
        (Policy::TokenCount(_), result) => result.transpose().map_err(|error| {
            unsupported(&format!("constant token arithmetic overflow: {error}"))
        })?,
        (Policy::EffectAmount | Policy::CounterAmount, result) => result.and_then(Result::ok),
    };
    let first = checked
        .first()
        .ok_or_else(|| unsupported("empty token arithmetic proof"))?
        .bounds;
    let bounds = (|| {
        if operation == Arithmetic::HalfUp {
            Ok(Bounds {
                lower: values::arithmetic(operation, &[first.lower])?,
                upper: values::arithmetic(operation, &[first.upper])?,
            })
        } else {
            checked.iter().skip(1).try_fold(first, |left, right| {
                left.binary(operation, right.bounds)
                    .ok_or_else(|| unsupported("token arithmetic proof operands"))
            })
        }
    })();
    let bounds = match (policy, bounds) {
        (Policy::TokenCount(_), value) => value?,
        (Policy::EffectAmount | Policy::CounterAmount, value) => value.unwrap_or(Bounds {
            lower: i64::MIN,
            upper: i64::MAX,
        }),
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

fn boolean(expression: &Expr<Bool>, policy: Policy) -> Result<CheckedBoolean> {
    let (plan, constant) = match expression {
        Expr::Literal(Bool(value)) => (Boolean::Literal(*value), Some(*value)),
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::And { predicates } | Predicate::Or { predicates } => {
                let checked = predicates
                    .iter()
                    .map(|value| boolean(value, policy))
                    .collect::<Result<Vec<_>>>()?;
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
                let checked = boolean(predicate, policy)?;
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
                let left = integer(left, policy)?;
                let right = integer(right, policy)?;
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
                let left = natural(left, policy)?;
                let right = natural(right, policy)?;
                let constant = left
                    .constant
                    .zip(right.constant)
                    .map(|(left, right)| values::compare(left, *comparison, right));
                (
                    Boolean::CompareNat(Box::new(left.plan), *comparison, Box::new(right.plan)),
                    constant,
                )
            }
            _ if policy.opaque_leaves() => (Boolean::Effect(Box::new(expression.clone())), None),
            _ => return Err(unsupported("token count predicate")),
        },
        _ if policy.opaque_leaves() => (Boolean::Effect(Box::new(expression.clone())), None),
        _ => return Err(unsupported("token count boolean reader")),
    };
    Ok(CheckedBoolean { plan, constant })
}

impl Natural {
    fn evaluate(&self, execution: &Execution, game: &Game) -> Result<u64> {
        match self {
            Self::Literal(value) => Ok(*value),
            Self::PaidX(binding) => execution
                .natural(game, &Expr::Bound(*binding))
                .map(|value| value.0),
            Self::PublicZone(value) | Self::Effect(value) => {
                execution.natural(game, value).map(|value| value.0)
            }
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
            Self::Literal(_) | Self::PaidX(_) | Self::PublicZone(_) | Self::Effect(_) => {}
        }
    }
}

impl Integer {
    fn evaluate(&self, execution: &Execution, game: &Game) -> Result<i64> {
        match self {
            Self::Literal(value) => Ok(*value),
            Self::Effect(value) => execution.integer(game, value).map(|value| value.0),
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
            Self::Literal(_) | Self::Effect(_) => {}
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
            Self::Literal(_) | Self::Effect(_) => {}
        }
    }

    fn evaluate(&self, execution: &Execution, game: &Game) -> Result<bool> {
        match self {
            Self::Literal(value) => Ok(*value),
            Self::Effect(value) => execution.boolean(game, value).map(|value| value.0),
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
