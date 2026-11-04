//! An equipment observes its own completed attachment and retains that occurrence's holder.
use crate::ir::{
    Binding, Bool, EquipEvent, EventPattern, Expr, ObjectFrozen, ObjectRef, Predicate,
};

pub(crate) fn predicate(event: Binding<EquipEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::ObjectEquals {
        left: Expr::Frozen(Box::new(ObjectFrozen::EquipEquipment { event })),
        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
    }))
}

pub(crate) fn binding(pattern: &EventPattern) -> Option<Binding<EquipEvent>> {
    if let EventPattern::Equip {
        binding,
        predicate: condition,
    } = pattern
        && *condition == predicate(*binding)
    {
        Some(*binding)
    } else {
        None
    }
}

pub(crate) fn holder(event: Binding<EquipEvent>) -> Expr<ObjectRef> {
    Expr::Frozen(Box::new(ObjectFrozen::EquipHolder { event }))
}
