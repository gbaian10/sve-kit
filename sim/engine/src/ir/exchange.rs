//! The exchange selector grammar is shared by structural and runtime checks.
use super::{
    Action, Bool, ChoiceDomain, ClassId, ClassRead, Expr, Info, MoveDestination, MoveReason, Nat,
    ObjectOperand, ObjectView, OwnerZone, PlayCostExchange, Posture, PostureRead, Predicate,
    Selector, ZoneKind,
};

/// A PP increase modifies the single play quotation and has no material route.
pub(crate) const fn pp_increase(exchange: &PlayCostExchange) -> bool {
    exchange.materials.is_empty()
        && exchange.route.is_none()
        && matches!(&exchange.cost, super::CostSpec::Ordered { steps } if steps.is_empty())
        && matches!(&exchange.change, super::CostChange::Delta { amount: Expr::Literal(super::Int(value)) } if *value > 0)
}

pub(crate) fn predicate(selector: &Selector) -> Option<(&Expr<Bool>, bool)> {
    let Selector::Zone {
        zone,
        binding,
        predicate,
        ..
    } = selector
    else {
        return None;
    };
    let expected = Expr::Compute(Box::new(Predicate::PostureEquals {
        left: Expr::Live(Box::new(PostureRead::Posture {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::Bound { binding: *binding }),
            },
        })),
        right: Expr::Literal(Posture::Standing),
    }));
    if *zone == ZoneKind::Field
        && let Expr::Compute(operation) = predicate.as_ref()
        && let Predicate::And { predicates } = operation.as_ref()
        && let [standing, rest] = predicates.as_slice()
        && *standing == expected
    {
        return Some((rest, true));
    }
    Some((predicate, false))
}

fn class(selector: &Selector) -> Option<&ClassId> {
    let Selector::Zone { binding, .. } = selector else {
        return None;
    };
    let (expression, _) = predicate(selector)?;
    let Expr::Compute(operation) = expression else {
        return None;
    };
    let Predicate::ClassEquals { left, right } = operation.as_ref() else {
        return None;
    };
    let Expr::Live(read) = left else {
        return None;
    };
    let ClassRead::Class {
        object: ObjectView::Current { object },
        view: super::CharacteristicView::Effective,
    } = read.as_ref()
    else {
        return None;
    };
    if !matches!(object.as_ref(), ObjectOperand::Bound { binding: current } if current == binding) {
        return None;
    }
    let Expr::Literal(Info::Present(class)) = right else {
        return None;
    };
    (!class.0.is_empty()).then_some(class)
}

pub(crate) fn pair(exchange: &PlayCostExchange) -> bool {
    let Some(routes) = exchange.routes() else {
        return false;
    };
    if routes.len() != 2 {
        return false;
    }
    let mut common: Option<(&ClassId, Nat)> = None;
    let mut field = false;
    let mut ex = false;
    for (decision, step) in routes {
        let ChoiceDomain::Objects { constraint, .. } = &decision.domain else {
            return false;
        };
        let [group] = constraint.groups.as_slice() else {
            return false;
        };
        let Selector::Zone { zone, .. } = &group.selector else {
            return false;
        };
        let Some((_, standing)) = predicate(&group.selector) else {
            return false;
        };
        let Some(class) = class(&group.selector) else {
            return false;
        };
        let current = (class, constraint.cardinality.minimum);
        if common.is_some_and(|prior| prior != current) {
            return false;
        }
        common = Some(current);
        match (zone, standing, &step.instruction.action) {
            (ZoneKind::Field, true, Action::Move { .. }) if !field => field = true,
            (ZoneKind::Ex, false, Action::Banish { .. }) if !ex => ex = true,
            _ => return false,
        }
    }
    field && ex
}

/// Tokens may pay only the existing own-Field Put route into their owners' cemetery.
pub(crate) fn field_put_route(exchange: &PlayCostExchange, index: usize) -> bool {
    let Some(routes) = exchange.routes() else {
        return false;
    };
    let Some((decision, step)) = routes.get(index) else {
        return false;
    };
    let ChoiceDomain::Objects {
        constraint,
        binding,
        ..
    } = &decision.domain
    else {
        return false;
    };
    matches!(constraint.groups.as_slice(), [group]
        if matches!(group.selector, Selector::Zone { zone: ZoneKind::Field, .. }))
        && matches!(&step.instruction.action,
            Action::Move { objects: Selector::Bound { reference }, movement, reason: MoveReason::Put }
                if reference == binding
                    && movement.to == (MoveDestination::Owners { zone: OwnerZone::Cemetery }))
}
