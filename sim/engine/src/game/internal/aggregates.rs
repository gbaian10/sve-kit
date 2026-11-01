use super::{Execution, Game, Result, invalid, unsupported};
use crate::ir::{
    Binding, Bool, CardKind, CharacteristicView, Expr, ExtremumKind, KindRead, NumericProperty,
    ObjectOperand, ObjectRef, ObjectView, Predicate, SelectionView, Selector, Stat, ZoneKind,
};

pub(super) fn check(objects: &Selector, property: NumericProperty) -> Result<()> {
    match objects {
        Selector::Zone {
            zone,
            view: SelectionView::Current {},
            binding,
            predicate,
            ..
        } if matches!(zone, ZoneKind::Field | ZoneKind::Ex | ZoneKind::Cemetery) => {
            super::card::selection::public_zone_selector(objects)?;
            if matches!(property, NumericProperty::Attack | NumericProperty::Life)
                && (*zone != ZoneKind::Field || !follower(predicate, *binding))
            {
                return Err(unsupported(
                    "aggregate stats require public field followers",
                ));
            }
            Ok(())
        }
        Selector::Difference { input, except }
            if matches!(except.as_ref(), Selector::Objects { objects: excluded }
                if excluded == &[ObjectOperand::SourceHolder {}]) =>
        {
            check(input, property)
        }
        _ => Err(unsupported("public aggregate input")),
    }
}

fn follower(predicate: &Expr<Bool>, candidate: Binding<ObjectRef>) -> bool {
    match predicate {
        Expr::Compute(operation) => match operation.as_ref() {
            Predicate::KindEquals {
                left: Expr::Live(read),
                right: Expr::Literal(CardKind::Follower | CardKind::EvolveFollower),
            } if matches!(read.as_ref(), KindRead::Kind {
                    object: ObjectView::Current { object }, view: CharacteristicView::Effective
                } if matches!(object.as_ref(), ObjectOperand::Bound { binding } if *binding == candidate)) =>
            {
                true
            }
            Predicate::And { predicates } => {
                predicates.iter().any(|item| follower(item, candidate))
            }
            Predicate::Or { predicates } => {
                !predicates.is_empty() && predicates.iter().all(|item| follower(item, candidate))
            }
            _ => false,
        },
        _ => false,
    }
}

fn values(
    execution: &Execution,
    game: &Game,
    objects: &Selector,
    property: NumericProperty,
) -> Result<Vec<(ObjectRef, i64)>> {
    check(objects, property)?;
    execution
        .objects(game, objects)?
        .into_iter()
        .map(|reference| {
            let id = execution.object_id(reference)?;
            let object = game.object(id)?;
            if object.generation != reference.generation.0
                || !matches!(object.zone.as_str(), "field" | "ex" | "cemetery")
            {
                return Err(unsupported("public current aggregate member"));
            }
            let value = match property {
                NumericProperty::OriginalCost => {
                    super::cost::printed_cost(&game.internal_cost_print(id)?["cost"])?
                }
                NumericProperty::Cost => game.internal_effective_cost(id)?,
                NumericProperty::Attack => game.internal_numeric_stat(id, Stat::Attack)?,
                NumericProperty::Life => game.internal_numeric_stat(id, Stat::Life)?,
            };
            Ok((reference, value))
        })
        .collect()
}

pub(super) fn sum(
    execution: &Execution,
    game: &Game,
    objects: &Selector,
    property: NumericProperty,
) -> Result<i64> {
    values(execution, game, objects, property)?
        .into_iter()
        .try_fold(0_i64, |sum, (_, value)| {
            sum.checked_add(value)
                .ok_or_else(|| invalid("aggregate sum overflow"))
        })
}

pub(super) fn extremum(
    execution: &Execution,
    game: &Game,
    objects: &Selector,
    property: NumericProperty,
    kind: ExtremumKind,
) -> Result<Vec<ObjectRef>> {
    let members = values(execution, game, objects, property)?;
    let numbers = members.iter().map(|(_, value)| *value);
    let limit = match kind {
        ExtremumKind::Minimum => numbers.min(),
        ExtremumKind::Maximum => numbers.max(),
    };
    Ok(members
        .into_iter()
        .filter_map(|(reference, value)| (Some(value) == limit).then_some(reference))
        .collect())
}
