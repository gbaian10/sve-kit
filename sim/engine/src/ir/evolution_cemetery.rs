//! The closed original-cost cemetery qualification shared by lowering and admission.
use super::{
    Binding, Bool, Cardinality, CharacteristicView, Comparison, Expr, Int, IntOperation, Nat,
    NatRead, NumberRead, ObjectOperand, ObjectRef, ObjectView, PlayerRef, PlayerRelation,
    PlayerSet, Predicate, SelectionView, Selector, TextQuantifier, ZoneKind,
};

pub(crate) fn predicate(cost: i64, minimum: i64, binding: Binding<ObjectRef>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::CompareInt {
        left: Expr::Compute(Box::new(IntOperation::FromNat {
            value: Expr::Live(Box::new(NatRead::ZoneCount {
                selector: Selector::Zone {
                    players: PlayerSet {
                        relation: PlayerRelation::SelfPlayer,
                        relative_to: PlayerRef::Controller {},
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        text_quantifier: TextQuantifier::Implicit,
                    },
                    zone: ZoneKind::Cemetery,
                    view: SelectionView::Current {},
                    binding,
                    predicate: Box::new(Expr::Compute(Box::new(Predicate::CompareInt {
                        left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                            information: NumberRead::Cost {
                                object: ObjectView::Current {
                                    object: Box::new(ObjectOperand::Bound { binding }),
                                },
                                view: CharacteristicView::Original,
                            },
                        })),
                        comparison: Comparison::Equal,
                        right: Expr::Literal(Int(cost)),
                    }))),
                },
            })),
        })),
        comparison: Comparison::GreaterEqual,
        right: Expr::Literal(Int(minimum)),
    }))
}

pub(crate) fn parameters(value: &Expr<Bool>) -> Option<(i64, i64)> {
    let Expr::Compute(operation) = value else {
        return None;
    };
    let Predicate::CompareInt {
        left: Expr::Compute(count),
        right: Expr::Literal(Int(minimum)),
        ..
    } = operation.as_ref()
    else {
        return None;
    };
    let IntOperation::FromNat {
        value: Expr::Live(read),
    } = count.as_ref()
    else {
        return None;
    };
    let NatRead::ZoneCount {
        selector:
            Selector::Zone {
                binding,
                predicate: filter,
                ..
            },
    } = read.as_ref()
    else {
        return None;
    };
    let Expr::Compute(filter) = filter.as_ref() else {
        return None;
    };
    let Predicate::CompareInt {
        right: Expr::Literal(Int(cost)),
        ..
    } = filter.as_ref()
    else {
        return None;
    };
    // Equality against the complete grammar also checks the bound object, view and owner.
    (*cost >= 0 && *minimum >= 0 && *value == predicate(*cost, *minimum, *binding))
        .then_some((*cost, *minimum))
}
