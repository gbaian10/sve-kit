//! Event identity stays frozen; references to an entering card read its current or last field values.
use super::subject::SubjectFields;
use crate::ir::{
    CharacteristicView, EventContext, EventSnapshotPolicy, Expr, Int, IntOperation, LkiReason,
    NumberRead, ObjectFrozen, ObjectOperand, ObjectRef, ObjectView, ScopeId, Stat,
};

pub(crate) fn view(fields: SubjectFields) -> Option<ObjectView> {
    let object = Box::new(ObjectOperand::FromExpression {
        value: Box::new(fields.object()),
    });
    match fields {
        SubjectFields::Entry(_) => Some(ObjectView::LastKnown {
            object,
            reason: LkiReason::ZoneExit,
        }),
        SubjectFields::Leave(event)
        | SubjectFields::Cemetery(event)
        | SubjectFields::Hand(event) => Some(ObjectView::EventSnapshot {
            object,
            event: EventContext::LeaveField { event },
            policy: EventSnapshotPolicy::BeforeExit,
        }),
        SubjectFields::Attack(_)
        | SubjectFields::Posture(_)
        | SubjectFields::DamageSource(_)
        | SubjectFields::Played { .. }
        | SubjectFields::Destroy(_)
        | SubjectFields::Evolve(_)
        | SubjectFields::Ex(_)
        | SubjectFields::Ub(_)
        | SubjectFields::Race(_) => None,
    }
}

fn fields(object: &ObjectView) -> Option<SubjectFields> {
    let fields = if let ObjectView::LastKnown {
        object,
        reason: LkiReason::ZoneExit,
    } = object
    {
        let ObjectOperand::FromExpression { value } = object.as_ref() else {
            return None;
        };
        let Expr::Frozen(value) = value.as_ref() else {
            return None;
        };
        let ObjectFrozen::EnterFieldObject { event } = value.as_ref() else {
            return None;
        };
        SubjectFields::Entry(*event)
    } else if let ObjectView::EventSnapshot {
        event: EventContext::LeaveField { event },
        ..
    } = object
    {
        SubjectFields::Leave(*event)
    } else {
        return None;
    };
    (view(fields).as_ref() == Some(object)).then_some(fields)
}

pub(crate) fn subject(object: &ObjectView) -> Option<Expr<ObjectRef>> {
    fields(object).map(SubjectFields::object)
}

pub(crate) fn key(object: &ObjectView) -> Option<(ScopeId, u64)> {
    let fields = fields(object)?;
    if let SubjectFields::Entry(event) = fields {
        Some(event.key())
    } else if let SubjectFields::Leave(event) = fields {
        Some(event.key())
    } else {
        None
    }
}

pub(crate) fn number(
    fields: SubjectFields,
    property: Option<Stat>,
    original: bool,
) -> Option<Expr<Int>> {
    let object = view(fields)?;
    let view = if original {
        CharacteristicView::Original
    } else {
        CharacteristicView::Effective
    };
    let information = match property {
        Some(property) => NumberRead::Stats {
            object,
            property,
            view,
        },
        None => NumberRead::Cost { object, view },
    };
    Some(Expr::Compute(Box::new(IntOperation::InformationOrZero {
        information,
    })))
}
