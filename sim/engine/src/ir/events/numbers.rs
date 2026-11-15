//! Numeric event views read the exact committed subject, independently of its later liveness.
use super::subject::SubjectFields;
use crate::ir::{
    CharacteristicView, EventContext, EventSnapshotPolicy, Expr, Int, IntOperation, NumberRead,
    ObjectOperand, ObjectRef, ObjectView, Stat,
};

pub(crate) fn view(fields: SubjectFields) -> Option<ObjectView> {
    let (event, policy) = match fields {
        SubjectFields::Entry(event) => (
            EventContext::EnterField { event },
            EventSnapshotPolicy::AfterEntry,
        ),
        SubjectFields::Leave(event)
        | SubjectFields::Cemetery(event)
        | SubjectFields::Hand(event) => (
            EventContext::LeaveField { event },
            EventSnapshotPolicy::BeforeExit,
        ),
        SubjectFields::Attack(_)
        | SubjectFields::Posture(_)
        | SubjectFields::DamageSource(_)
        | SubjectFields::Played { .. }
        | SubjectFields::Destroy(_)
        | SubjectFields::Evolve(_)
        | SubjectFields::Ex(_)
        | SubjectFields::Ub(_) => return None,
    };
    Some(ObjectView::EventSnapshot {
        object: Box::new(ObjectOperand::FromExpression {
            value: Box::new(fields.object()),
        }),
        event,
        policy,
    })
}

pub(crate) fn subject(object: &ObjectView) -> Option<Expr<ObjectRef>> {
    let ObjectView::EventSnapshot { event, .. } = object else {
        return None;
    };
    let fields = if let EventContext::EnterField { event } = event {
        SubjectFields::Entry(*event)
    } else if let EventContext::LeaveField { event } = event {
        SubjectFields::Leave(*event)
    } else {
        return None;
    };
    (view(fields).as_ref() == Some(object)).then(|| fields.object())
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
    let information = property.map_or_else(
        || NumberRead::Cost {
            object: object.clone(),
            view,
        },
        |property| NumberRead::Stats {
            object: object.clone(),
            property,
            view,
        },
    );
    Some(Expr::Compute(Box::new(IntOperation::InformationOrZero {
        information,
    })))
}
