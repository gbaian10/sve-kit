//! Observer filters read only the committed race subject and controller.
use super::subject::{SubjectFields, SubjectFilter};
use crate::ir::{Ability, AbilityOrigin, EventPattern, TriggerCondition};
pub(crate) fn filter(ability: &Ability) -> Option<SubjectFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. }) {
        return None;
    }
    let TriggerCondition::Event {
        pattern: EventPattern::Race { binding, predicate },
    } = &ability.trigger.as_ref()?.on
    else {
        return None;
    };
    SubjectFilter::parse(SubjectFields::Race(*binding), predicate)
}
