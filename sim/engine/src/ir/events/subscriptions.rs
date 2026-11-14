//! Alternative subscriptions route to their existing event producers with one ability identity.
use crate::ir::events::fusion;
use crate::ir::{Ability, EventPattern, EventSnapshotPolicy, TriggerCondition};

pub(crate) const fn view(pattern: &EventPattern) -> EventSnapshotPolicy {
    match pattern {
        EventPattern::AnyOf { .. }
        | EventPattern::Play { .. }
        | EventPattern::UbActivated { .. } => EventSnapshotPolicy::AtReveal,
        EventPattern::Fused { .. }
        | EventPattern::Move { .. }
        | EventPattern::LeaveField { .. }
        | EventPattern::Destroy { .. }
        | EventPattern::Discard { .. }
        | EventPattern::Banish { .. }
        | EventPattern::EarthRitePayment { .. }
        | EventPattern::Payment { .. } => EventSnapshotPolicy::BeforeExit,
        EventPattern::Targeted { .. }
        | EventPattern::Resolve { .. }
        | EventPattern::EnterField { .. }
        | EventPattern::EnterEx { .. }
        | EventPattern::Draw { .. }
        | EventPattern::Damage { .. }
        | EventPattern::StatChange { .. }
        | EventPattern::LifeChange { .. }
        | EventPattern::CounterChange { .. }
        | EventPattern::Posture { .. }
        | EventPattern::Evolve { .. }
        | EventPattern::Attack { .. }
        | EventPattern::Phase { .. }
        | EventPattern::Race { .. }
        | EventPattern::DriveGained { .. }
        | EventPattern::DriveTrigger { .. }
        | EventPattern::Equip { .. }
        | EventPattern::Dice { .. }
        | EventPattern::DriveChecked { .. } => EventSnapshotPolicy::AfterEntry,
    }
}

pub(crate) fn alternatives(ability: &Ability) -> Vec<Ability> {
    let Some(trigger) = &ability.trigger else {
        return vec![ability.clone()];
    };
    let TriggerCondition::Event {
        pattern: EventPattern::AnyOf { patterns },
    } = &trigger.on
    else {
        return vec![ability.clone()];
    };
    if fusion::combined(&EventPattern::AnyOf {
        patterns: patterns.clone(),
    }) {
        return vec![ability.clone()];
    }
    let mut unique = Vec::new();
    for alternative in patterns.iter().flat_map(|pattern| {
        let mut branch = ability.clone();
        if let Some(branch_trigger) = &mut branch.trigger {
            branch_trigger.on = TriggerCondition::Event {
                pattern: pattern.clone(),
            };
            branch_trigger.view = view(pattern);
        }
        alternatives(&branch)
    }) {
        // Repeated alternatives are one subscription; extra occurrences are added by producers.
        if !unique.contains(&alternative) {
            unique.push(alternative);
        }
    }
    unique
}

pub(crate) fn compound(ability: &Ability) -> bool {
    matches!(
        ability.trigger.as_ref().map(|trigger| &trigger.on),
        Some(TriggerCondition::Event {
            pattern: EventPattern::AnyOf { .. }
        })
    ) && !matches!(ability.trigger.as_ref().map(|trigger| &trigger.on),
            Some(TriggerCondition::Event { pattern }) if fusion::combined(pattern))
}
