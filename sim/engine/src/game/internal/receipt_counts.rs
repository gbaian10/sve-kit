use super::{Execution, Fact, Game, InstructionOutcome, Result, invalid, unsupported};
use crate::ir::{
    Binding, Bool, CharacteristicView, Characteristics, ClassRead, Comparison, Expr, Info, Int,
    IntOperation, KindRead, MemberProjection, MovementReceipt, NamesRead, NumberRead,
    ObjectReceipt, ObjectRef, ObjectView, Predicate, ReceiptLocation, RuntimeObjectView,
    TraitsRead,
};
use core::convert::identity;

pub(super) fn check(
    receipt: &ObjectReceipt,
    projection: MemberProjection,
    member: Binding<RuntimeObjectView>,
    predicate: &Expr<Bool>,
) -> Result<()> {
    let ObjectReceipt::Movement {
        receipt: MovementReceipt::Discard { receipt },
    } = receipt
    else {
        return Err(unsupported("discard member condition"));
    };
    if projection != MemberProjection::Before
        || !matches!(receipt.location(), ReceiptLocation::Local { .. })
    {
        return Err(unsupported("discard member projection"));
    }
    matches(predicate, member, None, None).map(|_| ())
}

pub(super) fn any(
    game: &Game,
    execution: &Execution,
    receipt: &ObjectReceipt,
    projection: MemberProjection,
    member: Binding<RuntimeObjectView>,
    predicate: &Expr<Bool>,
) -> Result<bool> {
    check(receipt, projection, member, predicate)?;
    let ObjectReceipt::Movement {
        receipt: MovementReceipt::Discard { receipt },
    } = receipt
    else {
        return Err(unsupported("discard member condition"));
    };
    let ReceiptLocation::Local { id } = receipt.location() else {
        return Err(unsupported("iterated discard condition"));
    };
    let Some(InstructionOutcome::Discard(outcome)) = execution.receipts.get(id) else {
        return Err(invalid("missing discard producer"));
    };
    for result in &outcome.successful {
        let before = result
            .before
            .ok_or_else(|| invalid("missing discard before member"))?;
        let characters = execution
            .facts
            .iter()
            .find_map(|fact| match fact {
                Fact::Discard(event) if event.card_before == before.object => {
                    Some(&event.characters)
                }
                _ => None,
            })
            .ok_or_else(|| invalid("missing discarded characteristics"))?;
        if matches(
            predicate,
            member,
            Some(characters),
            Some((game, execution, before.object)),
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn snapshot(object: &ObjectView, member: Binding<RuntimeObjectView>) -> Result<()> {
    if !matches!(object,ObjectView::Snapshot{value} if value.as_ref()==&Expr::Bound(member)) {
        return Err(unsupported("receipt member snapshot"));
    }
    Ok(())
}

fn matches(
    expression: &Expr<Bool>,
    member: Binding<RuntimeObjectView>,
    characters: Option<&Characteristics>,
    original: Option<(&Game, &Execution, ObjectRef)>,
) -> Result<bool> {
    let Expr::Compute(predicate) = expression else {
        return match expression {
            Expr::Literal(Bool(value)) => Ok(*value),
            _ => Err(unsupported("receipt member predicate")),
        };
    };
    match predicate.as_ref() {
        Predicate::And { predicates } | Predicate::Or { predicates } => {
            // Validate every branch even when an earlier value determines the result.
            let values = predicates
                .iter()
                .map(|item| matches(item, member, characters, original))
                .collect::<Result<Vec<_>>>()?;
            Ok(if matches!(predicate.as_ref(), Predicate::And { .. }) {
                values.into_iter().all(identity)
            } else {
                values.into_iter().any(identity)
            })
        }
        Predicate::Not { predicate } => {
            matches(predicate, member, characters, original).map(|value| !value)
        }
        Predicate::CompareInt {
            left: Expr::Compute(operation),
            comparison,
            right: Expr::Literal(Int(threshold)),
        } if *threshold >= 0 && *comparison == Comparison::GreaterEqual => {
            original_cost_matches(operation, member, *threshold, original)
        }
        Predicate::KindEquals {
            left: Expr::Live(read),
            right: Expr::Literal(expected),
        } => {
            let KindRead::Kind { object, .. } = read.as_ref();
            snapshot(object, member)?;
            Ok(characters.is_some_and(|characters| characters.kind == *expected))
        }
        Predicate::ClassEquals {
            left: Expr::Live(read),
            right: Expr::Literal(expected),
        } => {
            let ClassRead::Class { object, .. } = read.as_ref();
            snapshot(object, member)?;
            Ok(
                characters.is_some_and(|characters| match (&characters.class, expected) {
                    (Some(actual), Info::Present(expected)) => actual == expected,
                    (None, Info::AbsentByRules) => true,
                    _ => false,
                }),
            )
        }
        Predicate::HasTrait {
            traits: Expr::Live(read),
            trait_id,
        } => {
            let TraitsRead::Traits { object, .. } = read.as_ref();
            snapshot(object, member)?;
            Ok(characters.is_some_and(|characters| characters.traits.contains(trait_id)))
        }
        Predicate::NamesIntersect {
            left: Expr::Live(read),
            right: Expr::Literal(expected),
        } => {
            let NamesRead::Names { object } = read.as_ref();
            snapshot(object, member)?;
            Ok(characters.is_some_and(|characters| {
                characters.names.iter().any(|name| expected.contains(name))
            }))
        }
        Predicate::NameContains {
            object,
            name: Expr::Literal(name),
        } => {
            snapshot(object, member)?;
            Ok(characters.is_some_and(|characters| {
                characters
                    .names
                    .iter()
                    .any(|candidate| candidate.0.contains(&name.0))
            }))
        }
        _ => Err(unsupported("discard member predicate shape")),
    }
}

fn original_cost_matches(
    operation: &IntOperation,
    member: Binding<RuntimeObjectView>,
    threshold: i64,
    original: Option<(&Game, &Execution, ObjectRef)>,
) -> Result<bool> {
    let IntOperation::InformationOrZero {
        information:
            NumberRead::Cost {
                object,
                view: CharacteristicView::Original,
            },
    } = operation
    else {
        return Err(unsupported("discard original cost comparison"));
    };
    snapshot(object, member)?;
    let Some((game, execution, before)) = original else {
        return Ok(false);
    };
    // The immutable base printing survives movement and effective-cost changes.
    let card = &game.object(execution.object_id(before)?)?.card;
    let cost = super::cost::printed_cost(&game.catalog.face(card, 0)?["cost"])?;
    Ok(cost >= threshold)
}
