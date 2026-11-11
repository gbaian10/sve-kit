//! Admission is limited to the suppression lifetimes with turn cleanup.
use super::{
    Action, DurationEnd, ExecutionRef, ObjectOperand, RemoveScope, Selector, TimeAnchor,
    TimestampBasis,
};

pub(crate) fn supported(action: &Action) -> bool {
    let Action::RemoveAbility {
        subjects,
        scope: RemoveScope::AllAtTime {},
        duration,
        timestamp,
    } = action
    else {
        return false;
    };
    let singleton = matches!(subjects, Selector::Bound { .. })
        || matches!(subjects, Selector::Objects { objects } if matches!(
            objects.as_slice(), [ObjectOperand::SourceHolder {} | ObjectOperand::Selected { .. }]
        ));
    singleton
        && matches!(
            duration.end,
            DurationEnd::NoTemporalExpiry {}
                | DurationEnd::CurrentTurnEnd {}
                | DurationEnd::NextTurnEnd { .. }
        )
        && duration.anchor == (TimeAnchor::Installation {})
        && duration.usage.is_none()
        && duration.intervals.is_empty()
        && *timestamp
            == (TimestampBasis::ProperPlay {
                execution: ExecutionRef::Current {},
            })
}
