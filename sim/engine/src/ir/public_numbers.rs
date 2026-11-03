//! Closed candidate-independent public reads for declaration quantities and numeric limits.
#![expect(
    clippy::wildcard_enum_match_arm,
    reason = "Unlisted reads must stay outside the closed public declaration grammar."
)]
use super::{
    Arithmetic, Binding, Bool, CharacteristicView, ClassRead, Expr, Info, Int, IntOperation,
    IntRead, KindRead, NamesRead, NatRead, NumberRead, ObjectOperand, ObjectRef, ObjectView,
    PlayerRef, PlayerRelation, PlayerResource, Predicate, SelectionView, Selector, TraitsRead,
    ZoneKind,
};
pub(crate) fn check(value: &Expr<Int>) -> bool {
    match value {
        Expr::Literal(_) => true,
        Expr::Live(read) => matches!(
            read.as_ref(),
            IntRead::PlayerResource {
                player: PlayerRef::Controller {},
                property: PlayerResource::PpMaximum
            }
        ),
        Expr::Compute(operation) => match operation.as_ref() {
            IntOperation::Arithmetic {
                operation:
                    Arithmetic::Add
                    | Arithmetic::Subtract
                    | Arithmetic::Multiply
                    | Arithmetic::FloorDivide
                    | Arithmetic::Minimum,
                arguments,
            } => !arguments.is_empty() && arguments.iter().all(check),
            IntOperation::FromNat {
                value: Expr::Live(read),
            } => matches!(read.as_ref(),NatRead::ZoneCount {selector} if count(selector)),
            _ => false,
        },
        _ => false,
    }
}
fn count(selector: &Selector) -> bool {
    match selector {
        Selector::Difference { input, except } => {
            matches!(except.as_ref(),Selector::Objects{objects} if objects.as_slice()==[ObjectOperand::SourceHolder{}])
                && count(input)
        }
        Selector::Zone {
            players,
            zone: ZoneKind::Field | ZoneKind::Ex | ZoneKind::Cemetery,
            view: SelectionView::Current {},
            binding,
            predicate,
        } => {
            players.relation == PlayerRelation::SelfPlayer
                && players.relative_to == (PlayerRef::Controller {})
                && predicate_filter(predicate, *binding)
        }
        _ => false,
    }
}
fn candidate(view: &ObjectView, binding: Binding<ObjectRef>) -> bool {
    matches!(view,ObjectView::Current {object} if **object==(ObjectOperand::Bound {binding}))
}
fn predicate_filter(value: &Expr<Bool>, binding: Binding<ObjectRef>) -> bool {
    let Expr::Compute(operation) = value else {
        return *value == Expr::Literal(Bool(true));
    };
    match operation.as_ref() {
        Predicate::And { predicates } => predicates.iter().all(|p| predicate_filter(p, binding)),
        Predicate::Not { predicate } => predicate_filter(predicate, binding),
        Predicate::KindEquals {
            left: Expr::Live(read),
            right: Expr::Literal(_),
        } => {
            matches!(read.as_ref(),KindRead::Kind{object,view:CharacteristicView::Effective} if candidate(object,binding))
        }
        Predicate::ClassEquals {
            left: Expr::Live(read),
            right: Expr::Literal(Info::Present(_)),
        } => {
            matches!(read.as_ref(),ClassRead::Class{object,view:CharacteristicView::Original|CharacteristicView::Effective} if candidate(object,binding))
        }
        Predicate::HasTrait {
            traits: Expr::Live(read),
            ..
        } => {
            matches!(read.as_ref(),TraitsRead::Traits {object,view:CharacteristicView::Effective} if candidate(object,binding))
        }
        Predicate::NamesIntersect {
            left: Expr::Live(read),
            right: Expr::Literal(_),
        } => matches!(read.as_ref(),NamesRead::Names{object} if candidate(object,binding)),
        Predicate::CompareInt {
            left: Expr::Compute(operation),
            right: Expr::Literal(_),
            ..
        } => {
            matches!(operation.as_ref(),IntOperation::InformationOrZero{information:NumberRead::Cost{object,..}} if candidate(object,binding))
        }
        _ => false,
    }
}

/// The only arithmetic mode cap admitted here counts complete units of one public population.
pub(crate) fn per_count(value: &Expr<Int>) -> bool {
    let Expr::Compute(arithmetic) = value else {
        return false;
    };
    let IntOperation::Arithmetic {
        operation: Arithmetic::FloorDivide,
        arguments,
    } = arithmetic.as_ref()
    else {
        return false;
    };
    let [Expr::Compute(conversion), Expr::Literal(Int(unit))] = arguments.as_slice() else {
        return false;
    };
    *unit > 0
        && matches!(conversion.as_ref(), IntOperation::FromNat { value: Expr::Live(read) } if matches!(read.as_ref(), NatRead::ZoneCount { .. }))
        && check(value)
}
