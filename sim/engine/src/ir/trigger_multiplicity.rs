//! A live phase rule adds occurrences while retaining each original ability's source.
use super::events::timing::{opponent, phase_predicate};
use super::{
    AbilitySource, Binding, Bool, Duration, DurationEnd, Effect, EventPattern, Expr, Nat, Phase,
    PhaseEvent, PlayerRef, RuleModifier, TimeAnchor,
};

pub(crate) struct Multiplicity<'effect> {
    pub source: &'effect AbilitySource,
    pub condition: &'effect Expr<Bool>,
    pub controller: &'effect PlayerRef,
    pub phase: Phase,
    pub binding: Binding<PhaseEvent>,
    pub additional: Nat,
}

pub(crate) fn projection(effect: &Effect) -> Option<Multiplicity<'_>> {
    let Effect::RuleModifier {
        modifier:
            RuleModifier::TriggerMultiplicity {
                pattern: EventPattern::Phase { binding, predicate },
                controller,
                add,
            },
        condition,
        source,
        duration,
    } = effect
    else {
        return None;
    };
    if *duration
        != (Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        })
        || (*controller != (PlayerRef::Controller {}) && *controller != opponent())
    {
        return None;
    }
    [Phase::Main, Phase::End]
        .into_iter()
        .find(|phase| *predicate == phase_predicate(*binding, *phase, None))
        .map(|phase| Multiplicity {
            source,
            condition,
            controller,
            phase,
            binding: *binding,
            additional: *add,
        })
}
