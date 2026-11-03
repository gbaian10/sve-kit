//! UB subscriptions match the committed holder through the shared public subject plan.
use super::subject::{SubjectFields, SubjectFilter};
use crate::ir::{Ability, AbilityOrigin, CardKind, EventPattern, TriggerCondition, ZoneKind};

pub(crate) fn filter(ability: &Ability) -> Option<SubjectFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.active_zones != [ZoneKind::Field]
    {
        return None;
    }
    let trigger = ability.trigger.as_ref()?;
    let TriggerCondition::Event {
        pattern: EventPattern::UbActivated { binding, predicate },
    } = &trigger.on
    else {
        return None;
    };
    (trigger.active_zones == ability.active_zones).then_some(())?;
    let plan = subject();
    (*predicate == plan.predicate(SubjectFields::Ub(*binding))).then_some(plan)
}

pub(crate) fn subject() -> SubjectFilter {
    SubjectFilter {
        own: Some(true),
        itself: Some(false),
        kind: Some(CardKind::Follower),
        ..SubjectFilter::default()
    }
}
