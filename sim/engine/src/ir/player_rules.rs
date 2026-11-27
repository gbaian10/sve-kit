use super::phase_restrictions::affected_player;
use super::{
    AbilitySource, Bool, Duration, DurationEnd, Effect, Expr, Interval, PlayerRef, PlayerSet,
    RuleModifier, TimeAnchor,
};

#[derive(Clone, Copy)]
pub(crate) enum PlayerRule {
    UnlimitedHand,
    DoubleFanfare,
}

impl PlayerRule {
    pub(crate) const fn from_effect(effect: &Effect) -> Option<Self> {
        match effect {
            Effect::RuleModifier {
                modifier: RuleModifier::UnlimitedHand { .. },
                ..
            } => Some(Self::UnlimitedHand),
            Effect::RuleModifier {
                modifier: RuleModifier::DoubleFanfare { .. },
                ..
            } => Some(Self::DoubleFanfare),
            Effect::Continuous { .. }
            | Effect::Replacement { .. }
            | Effect::Permission { .. }
            | Effect::RuleModifier { .. }
            | Effect::DeckRule { .. }
            | Effect::CostModifier { .. } => None,
        }
    }

    pub(crate) fn specification(self, source: AbilitySource) -> (PlayerSet, Effect, Duration) {
        let player = PlayerRef::Controller {};
        let (modifier, end, intervals) = match self {
            Self::UnlimitedHand => (
                RuleModifier::UnlimitedHand {
                    player: player.clone(),
                },
                DurationEnd::NoTemporalExpiry {},
                vec![],
            ),
            Self::DoubleFanfare => (
                RuleModifier::DoubleFanfare {
                    player: player.clone(),
                },
                DurationEnd::NextTurnEnd {
                    player: player.clone(),
                },
                vec![Interval::DuringNextTurn {
                    player: player.clone(),
                }],
            ),
        };
        let duration = Duration {
            end,
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals,
        };
        let effect = Effect::RuleModifier {
            modifier,
            condition: Expr::Literal(Bool(true)),
            duration: duration.clone(),
            source,
        };
        (affected_player(player), effect, duration)
    }

    pub(crate) fn source<'effect>(
        self,
        players: &PlayerSet,
        effect: &'effect Effect,
        duration: &Duration,
    ) -> Option<&'effect AbilitySource> {
        let Effect::RuleModifier { source, .. } = effect else {
            return None;
        };
        let (expected_players, expected_effect, expected_duration) =
            self.specification(source.clone());
        (*players == expected_players
            && *effect == expected_effect
            && *duration == expected_duration)
            .then_some(source)
    }
}
