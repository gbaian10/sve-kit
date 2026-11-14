//! Live field-to-cemetery proposals redirected to banishment.
use super::{
    AbilitySource, Bool, Duration, DurationEnd, Effect, EventRewrite, Expr, MoveDestination,
    OncePerEvent, OwnerZone, PlayerRef, ProposedEventPattern, ReplacementScope, Selector,
    TimeAnchor, ZoneKind,
};

pub(crate) struct Replacement<'effect> {
    pub source: &'effect AbilitySource,
    pub objects: &'effect Selector,
    pub condition: &'effect Expr<Bool>,
}

pub(crate) fn projection(effect: &Effect) -> Option<Replacement<'_>> {
    let Effect::Replacement {
        source,
        pattern:
            ProposedEventPattern::Move {
                objects,
                from: ZoneKind::Field,
                to: ZoneKind::Cemetery,
            },
        condition,
        rewrite:
            EventRewrite::RedirectMove {
                to:
                    MoveDestination::Owners {
                        zone: OwnerZone::Banish,
                    },
                position: None,
            },
        scope: ReplacementScope::General,
        affected_player: PlayerRef::ProposedMoveController {},
        once_per_event: OncePerEvent::Enforced,
        usage: None,
        prospective_chars: None,
        duration,
        ..
    } = effect
    else {
        return None;
    };
    (*duration
        == Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        })
    .then_some(Replacement {
        source,
        objects,
        condition,
    })
}
