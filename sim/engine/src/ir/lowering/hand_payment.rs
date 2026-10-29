use super::{
    Allocator, Binding, Cardinality, LoweringError, Nat, PlayerRef, PlayerRelation, PlayerSet,
    TextQuantifier, ZoneKind, ast, search, unsupported,
};
use crate::ir::{
    Action, Bool, DiscardMode, Disjointness, Expr, ObjectRef, SelectionConstraint, SelectionGroup,
    SelectionView, Selector,
};

pub(super) fn lower(
    allocator: &mut Allocator,
    material: &ast::HandMaterial,
    path: &str,
) -> Result<Action, LoweringError> {
    if material.n == 0 {
        return Err(unsupported(path));
    }
    let binding = Binding::new(allocator.id(path)?, allocator.scope(path)?);
    let cardinality = Cardinality {
        minimum: Nat(material.n),
        maximum: Some(Nat(material.n)),
    };
    Ok(Action::Discard {
        player: PlayerRef::Controller {},
        mode: DiscardMode::Chosen {
            constraint: SelectionConstraint {
                groups: vec![SelectionGroup {
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
                        zone: ZoneKind::Hand,
                        view: SelectionView::Current {},
                        binding,
                        predicate: Box::new(filter(&material.sel, binding, path)?),
                    },
                    cardinality,
                }],
                cardinality,
                distinct_by: None,
                disjoint: Disjointness::Disjoint,
                aggregate_bounds: vec![],
            },
        },
    })
}

fn filter(
    query: &ast::HandQuery,
    binding: Binding<ObjectRef>,
    path: &str,
) -> Result<Expr<Bool>, LoweringError> {
    use crate::ir::{
        CharacteristicView, ClassId, ClassRead, Comparison, Info, Int, IntOperation, NumberRead,
        ObjectOperand, ObjectView, Predicate,
    };
    let view = ObjectView::Current {
        object: Box::new(ObjectOperand::Bound { binding }),
    };
    let base = search::object_filter(
        query.kind.as_ref(),
        query.r#trait.as_deref(),
        query.name.as_deref(),
        binding,
    );
    let mut predicates = if base == Expr::Literal(Bool(true)) {
        vec![]
    } else {
        vec![base]
    };
    if let Some(class) = &query.class {
        if class.is_empty() {
            return Err(unsupported(path));
        }
        predicates.push(Expr::Compute(Box::new(Predicate::ClassEquals {
            left: Expr::Live(Box::new(ClassRead::Class {
                object: view.clone(),
                view: CharacteristicView::Original,
            })),
            right: Expr::Literal(Info::Present(ClassId(class.clone()))),
        })));
    }
    for (amount, comparison) in [
        (query.orig_cost_eq, Comparison::Equal),
        (query.orig_cost_ge, Comparison::GreaterEqual),
        (query.orig_cost_le, Comparison::LessEqual),
    ] {
        if let Some(amount) = amount {
            predicates.push(Expr::Compute(Box::new(Predicate::CompareInt {
                left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                    information: NumberRead::Cost {
                        object: view.clone(),
                        view: CharacteristicView::Original,
                    },
                })),
                comparison,
                right: Expr::Literal(Int(
                    i64::try_from(amount).map_err(|_overflow| unsupported(path))?
                )),
            })));
        }
    }
    Ok(if predicates.is_empty() {
        Expr::Literal(Bool(true))
    } else {
        Expr::Compute(Box::new(Predicate::And { predicates }))
    })
}
