use super::{Action, Expr, LoweringError, Nat, ObjectOperand, ast, unsupported};
use crate::ir::{CounterKind, CounterOperation, Holder};

pub(super) const fn holder() -> Holder {
    Holder::Object {
        object: ObjectOperand::SourceHolder {},
    }
}

pub(super) fn change(value: &ast::CounterChange, path: &str) -> Result<Action, LoweringError> {
    let ast::This::This = value.on;
    let amount = value.delta.checked_abs().ok_or_else(|| unsupported(path))?;
    Ok(Action::Counter {
        holder: holder(),
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
