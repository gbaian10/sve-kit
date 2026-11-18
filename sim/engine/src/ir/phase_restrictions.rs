use super::{
    Action, ActionPattern, Bool, CardKind, Cardinality, Duration, DurationEnd, Edge, Effect, Expr,
    InstallationSubjects, InstallationTime, Interval, Nat, PermissionSubject, Phase, PlayerRef,
    PlayerRelation, PlayerSet, Polarity, RuleId, TextQuantifier, TimeAnchor,
};

#[derive(Clone, Copy)]
pub(crate) enum Restriction {
    Draw,
    MaximumPp,
    FollowerPlay,
}

impl Restriction {
    pub(crate) const fn phase(self) -> Phase {
        match self {
            Self::Draw | Self::MaximumPp => Phase::Start,
            Self::FollowerPlay => Phase::Main,
        }
    }

    fn action(self, player: PlayerRef) -> ActionPattern {
        match self {
            Self::Draw => ActionPattern::Draw { player },
            Self::MaximumPp => ActionPattern::IncreasePpMaximum { player },
            Self::FollowerPlay => ActionPattern::PlayCardKind {
                kind: CardKind::Follower,
            },
        }
    }

    pub(crate) fn duration(self, player: PlayerRef) -> Duration {
        let phase = self.phase();
        Duration {
            end: DurationEnd::NextPhaseEdge {
                player: player.clone(),
                phase,
                edge: Edge::End,
            },
            anchor: TimeAnchor::Installation {},
            intervals: vec![Interval::DuringNextPhase { player, phase }],
            usage: None,
        }
    }

    pub(crate) fn permission(self, players: PlayerSet, player: PlayerRef) -> Effect {
        Effect::Permission {
            subject: PermissionSubject::Players { players },
            action: self.action(player.clone()),
            polarity: Polarity::Forbid,
            condition: Expr::Literal(Bool(true)),
            duration: self.duration(player),
            rule: RuleId(0),
        }
    }
}

pub(crate) fn projection(action: &Action) -> Option<(Restriction, &PlayerRef)> {
    let Action::InstallEffect {
        subjects: InstallationSubjects::Players { players, .. },
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return None;
    };
    effect_window(players, effect, duration)
}

pub(crate) const fn affected_player(player: PlayerRef) -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: player,
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::Implicit,
    }
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only these fixed action patterns use this phase restriction projection."
)]
pub(crate) fn effect_window<'effect>(
    players: &PlayerSet,
    effect: &Effect,
    duration: &'effect Duration,
) -> Option<(Restriction, &'effect PlayerRef)> {
    let Effect::Permission { action, .. } = effect else {
        return None;
    };
    let restriction = match action {
        ActionPattern::Draw { .. } => Restriction::Draw,
        ActionPattern::IncreasePpMaximum { .. } => Restriction::MaximumPp,
        ActionPattern::PlayCardKind {
            kind: CardKind::Follower,
        } => Restriction::FollowerPlay,
        _ => return None,
    };
    let DurationEnd::NextPhaseEdge { player, .. } = &duration.end else {
        return None;
    };
    let allowed =
        matches!(player, PlayerRef::Controller {}) || *player == super::events::timing::opponent();
    (allowed
        && *players == affected_player(player.clone())
        && *duration == restriction.duration(player.clone())
        && *effect == restriction.permission(players.clone(), player.clone()))
    .then_some((restriction, player))
}
