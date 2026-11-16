use super::{Execution, Game, Result, invalid, unsupported};
use crate::ir::{
    Action, Binding, CostSpec, CounterKind, CounterOperation, Expr, Holder, Nat, ObjectOperand,
    Posture, Selector,
};
use alloc::collections::BTreeMap;

#[derive(Default)]
struct Budget {
    fixed: u64,
    copies: u64,
}

pub(super) fn maximum(
    game: &Game,
    execution: &Execution,
    binding: Binding<Nat>,
    costs: &[&CostSpec],
) -> Result<Option<u64>> {
    let mut totals: BTreeMap<String, Budget> = BTreeMap::new();
    let mut variable = false;
    let holder = Holder::Object {
        object: ObjectOperand::SourceHolder {},
    };
    for cost in costs {
        let CostSpec::Ordered { steps } = cost else {
            return Err(unsupported("counter cost alternatives"));
        };
        for step in steps {
            let Action::Counter {
                holder: paid_holder,
                kind,
                operation: CounterOperation::Remove {},
                amount,
            } = &step.instruction.action
            else {
                continue;
            };
            if paid_holder != &holder {
                continue;
            }
            variable |= matches!(amount, Expr::Bound(_));
            let total = totals.entry(game.catalog.keyword_id(&kind.0)).or_default();
            let fixed = match amount {
                Expr::Literal(Nat(value)) => Some(*value),
                Expr::Bound(parameter) if *parameter == binding => {
                    total.copies = total
                        .copies
                        .checked_add(1)
                        .ok_or_else(|| invalid("counter cost copies overflow"))?;
                    None
                }
                Expr::Bound(parameter) => match execution.slots.get(&parameter.key()) {
                    Some(super::Slot::Natural(Nat(value))) => Some(*value),
                    None => None,
                    _ => return Err(unsupported("counter cost parameter")),
                },
                _ => return Err(unsupported("counter cost amount")),
            };
            if let Some(value) = fixed {
                total.fixed = total
                    .fixed
                    .checked_add(value)
                    .ok_or_else(|| invalid("counter cost total overflow"))?;
            }
        }
    }
    let mut maximum: Option<u64> = None;
    for (kind, total) in totals {
        let available = execution
            .counter(game, &holder, &CounterKind(kind))?
            .0
            .checked_sub(total.fixed)
            .ok_or_else(|| {
                crate::EngineFailure::IllegalPlay("insufficient fixed counters before X".into())
            })?;
        if total.copies != 0 {
            let bound = available
                .checked_div(total.copies)
                .ok_or_else(|| invalid("counter cost copies"))?;
            maximum = Some(maximum.map_or(bound, |previous| previous.min(bound)));
        }
    }
    if variable {
        validate_source(game, execution, costs)?;
    }
    Ok(maximum)
}

fn validate_source(game: &Game, execution: &Execution, costs: &[&CostSpec]) -> Result<()> {
    let source = game.object(&execution.context.source)?;
    if !matches!(source.zone.as_str(), "field" | "ex")
        || source.controller != super::seat_name(execution.context.controller)?
    {
        return Err(crate::EngineFailure::IllegalPlay(
            "counter cost source changed".into(),
        ));
    }
    let mut standing = source.state["acted"] == false;
    for cost in costs {
        let CostSpec::Ordered { steps } = cost else {
            return Err(unsupported("counter cost alternatives"));
        };
        for step in steps {
            if let Action::Posture {
                objects: Selector::Objects { objects },
                posture: Posture::Acted,
            } = &step.instruction.action
                && objects == &[ObjectOperand::SourceHolder {}]
            {
                if !standing {
                    return Err(crate::EngineFailure::IllegalPlay(
                        "source cannot pay Act before X".into(),
                    ));
                }
                standing = false;
            }
        }
    }
    Ok(())
}
