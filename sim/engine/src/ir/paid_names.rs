//! Same-name selectors read only the before-members of declared card payments.
use super::{
    Binding, Bool, Expr, MemberProjection, MovementReceipt, NamesRead, ObjectOperand,
    ObjectReceipt, ObjectRef, ObjectView, Predicate, ReceiptId, ReceiptLocation, RuntimeObjectView,
};

pub(crate) struct Comparison<'expression> {
    pub receipt: &'expression ObjectReceipt,
    pub candidate: Binding<ObjectRef>,
}

pub(crate) const fn receipt_id(receipt: &ObjectReceipt) -> Option<ReceiptId> {
    if let ObjectReceipt::Reveal { receipt } = receipt {
        return match receipt.location() {
            ReceiptLocation::Local { id } => Some(*id),
            ReceiptLocation::Iteration { .. } => None,
        };
    }
    if let ObjectReceipt::Movement {
        receipt: MovementReceipt::Banish { receipt },
    } = receipt
    {
        return match receipt.location() {
            ReceiptLocation::Local { id } => Some(*id),
            ReceiptLocation::Iteration { .. } => None,
        };
    }
    None
}

pub(crate) fn predicate(
    receipt: ObjectReceipt,
    candidate: Binding<ObjectRef>,
    member: Binding<RuntimeObjectView>,
) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::ReceiptAny {
        receipt,
        projection: MemberProjection::Before,
        member,
        predicate: Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding: candidate }),
                },
            })),
            right: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Snapshot {
                    value: Box::new(Expr::Bound(member)),
                },
            })),
        })),
    }))
}

pub(crate) fn comparison(expression: &Expr<Bool>) -> Option<Comparison<'_>> {
    let Expr::Compute(operation) = expression else {
        return None;
    };
    let Predicate::ReceiptAny {
        receipt,
        projection: MemberProjection::Before,
        member,
        predicate: Expr::Compute(inner),
    } = operation.as_ref()
    else {
        return None;
    };
    receipt_id(receipt)?;
    let Predicate::NamesIntersect {
        left: Expr::Live(left),
        right: Expr::Live(right),
    } = inner.as_ref()
    else {
        return None;
    };
    let NamesRead::Names {
        object: ObjectView::Current { object },
    } = left.as_ref()
    else {
        return None;
    };
    let ObjectOperand::Bound { binding: candidate } = object.as_ref() else {
        return None;
    };
    matches!(right.as_ref(), NamesRead::Names { object: ObjectView::Snapshot { value } } if value.as_ref() == &Expr::Bound(*member))
        .then_some(Comparison { receipt, candidate: *candidate })
}
