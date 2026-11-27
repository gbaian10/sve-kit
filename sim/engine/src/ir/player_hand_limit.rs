use super::phase_restrictions::affected_player;
use super::{
    AbilitySource, Bool, Duration, DurationEnd, Effect, Expr, PlayerRef, PlayerSet, RuleModifier,
    TimeAnchor,
};

const fn duration() -> Duration {
    Duration {
        end: DurationEnd::NoTemporalExpiry {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    }
}

pub(crate) fn modifier(source: AbilitySource) -> (PlayerSet, Effect, Duration) {
    let player = PlayerRef::Controller {};
    let duration = duration();
    let effect = Effect::RuleModifier {
        modifier: RuleModifier::UnlimitedHand {
            player: player.clone(),
        },
        condition: Expr::Literal(Bool(true)),
        duration: duration.clone(),
        source,
    };
    (affected_player(player), effect, duration)
}

pub(crate) const fn is_modifier(effect: &Effect) -> bool {
    matches!(
        effect,
        Effect::RuleModifier {
            modifier: RuleModifier::UnlimitedHand { .. },
            ..
        }
    )
}

pub(crate) fn source<'effect>(
    players: &PlayerSet,
    effect: &'effect Effect,
    duration: &Duration,
) -> Option<&'effect AbilitySource> {
    let Effect::RuleModifier {
        modifier: RuleModifier::UnlimitedHand { .. },
        source,
        ..
    } = effect
    else {
        return None;
    };
    let (expected_players, expected_effect, expected_duration) = modifier(source.clone());
    (*players == expected_players && *effect == expected_effect && *duration == expected_duration)
        .then_some(source)
}
