//! Closed self-name declarations shared by lowering and runtime admission.
use super::{
    Ability, AbilityKind, AbilityOrigin, AbilityQualification, CardName, CharacteristicPatch,
    ContinuousLayer, Duration, DurationEnd, Effect, EffectTarget, ObjectOperand, PlayerRef,
    Selector, TimeAnchor, TimestampBasis, ZoneKind,
};

pub(crate) const ALL_ZONES: [ZoneKind; 12] = [
    ZoneKind::Deck,
    ZoneKind::Hand,
    ZoneKind::Field,
    ZoneKind::Ex,
    ZoneKind::Cemetery,
    ZoneKind::Banish,
    ZoneKind::Evolve,
    ZoneKind::Resolution,
    ZoneKind::Race,
    ZoneKind::Drive,
    ZoneKind::Trigger,
    ZoneKind::Equipment,
];

pub(crate) fn name(ability: &Ability) -> Option<&CardName> {
    if ability.kind != AbilityKind::Static
        || ability.qualification != AbilityQualification::Ordinary
        || !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
        || ability.play.is_some()
        || ability.trigger.is_some()
        || ability.body.is_some()
        || !ability.event_captures.is_empty()
        || !(ability.active_zones == [ZoneKind::Field] || ability.active_zones == ALL_ZONES)
    {
        return None;
    }
    let [
        Effect::Continuous {
            source,
            target: EffectTarget::Dynamic { selector },
            condition: None,
            patch: CharacteristicPatch::AddName { name },
            layer: ContinuousLayer::NonNumeric,
            timestamp,
            active_zones,
            duration,
            ..
        },
    ] = ability.declarations.as_slice()
    else {
        return None;
    };
    (source == &ability.source
        && active_zones == &ability.active_zones
        && !name.0.is_empty()
        && *selector
            == (Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            })
        && *timestamp
            == (TimestampBasis::ZoneEntry {
                object: ObjectOperand::SourceHolder {},
            })
        && *duration
            == (Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            }))
    .then_some(name)
}
