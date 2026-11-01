//! Played subscriptions narrow the committed card through the common subject plan.
use super::subject::{SubjectFields, SubjectFilter};
use crate::ir::{
    Ability, AbilityOrigin, Binding, Bool, EventPattern, Expr, ObjectRef, PlayEvent,
    PlaySubjectFields, Predicate, TriggerCondition, ZoneKind,
};

#[derive(Clone, Debug)]
pub(crate) struct PlayedFilter {
    pub(crate) subject: SubjectFilter,
    pub(crate) card: Binding<ObjectRef>,
}

impl PlayedFilter {
    pub(crate) fn predicate(&self, event: Binding<PlayEvent>) -> Expr<Bool> {
        Expr::Compute(Box::new(Predicate::PlayedCard {
            subject: Expr::Frozen(Box::new(PlaySubjectFields::PlaySubject { event })),
            card: self.card,
            predicate: self.subject.predicate(SubjectFields::Played {
                event,
                card: self.card,
            }),
        }))
    }

    pub(crate) fn parse(event: Binding<PlayEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let Expr::Compute(predicate) = expression else {
            return None;
        };
        let Predicate::PlayedCard {
            card, predicate, ..
        } = predicate.as_ref()
        else {
            return None;
        };
        let subject =
            SubjectFilter::parse(SubjectFields::Played { event, card: *card }, predicate)?;
        if subject
            .class
            .as_ref()
            .is_some_and(|class| class.0.is_empty())
        {
            return None;
        }
        let plan = Self {
            subject,
            card: *card,
        };
        (*expression == plan.predicate(event)).then_some(plan)
    }
}

pub(crate) fn filter(ability: &Ability) -> Option<PlayedFilter> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || !matches!(
            ability.active_zones.as_slice(),
            [ZoneKind::Field | ZoneKind::Cemetery]
        )
    {
        return None;
    }
    let trigger = ability.trigger.as_ref()?;
    let TriggerCondition::Event {
        pattern: EventPattern::Play { binding, predicate },
    } = &trigger.on
    else {
        return None;
    };
    (trigger.active_zones == ability.active_zones).then_some(())?;
    PlayedFilter::parse(*binding, predicate)
}
