//! The printed field aura relaxes only the controller's intrinsic evolution limit.
use super::{
    Ability, AbilityKind, AbilityOrigin, AbilityQualification, Bool, Duration, DurationEnd, Effect,
    Expr, ObjectOperand, PlayerRef, RuleModifier, TimeAnchor, ZoneKind,
};

pub(crate) fn is_static(ability: &Ability) -> bool {
    let [
        Effect::RuleModifier {
            modifier:
                RuleModifier::EvolutionPlayLimit {
                    player: PlayerRef::Controller {},
                },
            condition: Expr::Literal(Bool(true)),
            duration,
            source,
        },
    ] = ability.declarations.as_slice()
    else {
        return false;
    };
    ability.kind == AbilityKind::Static
        && ability.qualification == AbilityQualification::Ordinary
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && ability.source.holder == (ObjectOperand::SourceHolder {})
        && ability.source.controller == (PlayerRef::Controller {})
        && *source == ability.source
        && ability.active_zones == [ZoneKind::Field]
        && ability.event_captures.is_empty()
        && ability.play.is_none()
        && ability.trigger.is_none()
        && ability.body.is_none()
        && *duration
            == Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            }
}
