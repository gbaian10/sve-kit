//! A Fusion cost owns one selection across the controller's hand and EX.
use super::{
    Action, Cardinality, Disjointness, Nat, ObjectOperand, PlayerRef, PlayerRelation,
    SelectionConstraint, SelectionView, Selector, ZoneKind,
};

pub(crate) fn materials(action: &Action) -> Option<&SelectionConstraint> {
    let Action::Fusion {
        subject: ObjectOperand::SourceHolder {},
        materials,
    } = action
    else {
        return None;
    };
    let [group] = materials.groups.as_slice() else {
        return None;
    };
    let count = materials.cardinality;
    let fixed = count.minimum.0 > 0 && count.maximum == Some(count.minimum);
    let up_to_two = count.minimum == Nat(0) && count.maximum == Some(Nat(2));
    if group.cardinality != count
        || !(fixed || up_to_two)
        || materials.distinct_by.is_some()
        || materials.disjoint != Disjointness::Disjoint
        || !materials.aggregate_bounds.is_empty()
    {
        return None;
    }
    let Selector::Difference { input, except } = &group.selector else {
        return None;
    };
    if !matches!(except.as_ref(), Selector::Objects { objects } if objects == &[ObjectOperand::SourceHolder {}])
    {
        return None;
    }
    let Selector::Union { sets } = input.as_ref() else {
        return None;
    };
    if sets.len() != 2 {
        return None;
    }
    for (selector, expected) in sets.iter().zip([ZoneKind::Hand, ZoneKind::Ex]) {
        if !matches!(selector, Selector::Zone { players, zone, view: SelectionView::Current {}, .. }
            if *zone == expected
                && players.relative_to == (PlayerRef::Controller {})
                && players.relation == PlayerRelation::SelfPlayer
                && players.cardinality == (Cardinality { minimum: Nat(1), maximum: Some(Nat(1)) }))
        {
            return None;
        }
    }
    Some(materials)
}
