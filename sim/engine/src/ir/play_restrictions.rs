//! Printed permissions and restrictions checked before committing a card play.
use super::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource,
    ActionPattern, Bool, Duration, DurationEnd, Effect, Expr, ObjectOperand, PermissionSubject,
    PlayerRef, Polarity, RuleId, ScopeId, Selector, TimeAnchor, ZoneKind,
};
use super::{
    Action, Binding, Cardinality, InstallationSubjects, InstallationTime, Nat, ObjectRef,
    PlayerRelation, PlayerSet, SelectionView, TextQuantifier,
};

pub(crate) fn ability(
    source: AbilitySource,
    scope: ScopeId,
    active_zones: Vec<ZoneKind>,
    condition: Expr<Bool>,
) -> Ability {
    declaration(source, scope, active_zones, condition, Polarity::Forbid)
}

pub(crate) fn cemetery_ability(
    source: AbilitySource,
    scope: ScopeId,
    condition: Expr<Bool>,
) -> Ability {
    declaration(
        source,
        scope,
        vec![ZoneKind::Cemetery],
        condition,
        Polarity::Allow,
    )
}

fn declaration(
    source: AbilitySource,
    scope: ScopeId,
    active_zones: Vec<ZoneKind>,
    condition: Expr<Bool>,
    polarity: Polarity,
) -> Ability {
    let objects = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    Ability {
        id: AbilityId(scope.0),
        scope,
        source,
        kind: AbilityKind::Static,
        qualification: AbilityQualification::Ordinary,
        active_zones,
        event_captures: vec![],
        play: None,
        trigger: None,
        body: None,
        declarations: vec![Effect::Permission {
            subject: PermissionSubject::Objects {
                objects: objects.clone(),
            },
            action: ActionPattern::PlayCard { objects },
            polarity,
            condition,
            duration: Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            rule: RuleId(133),
        }],
    }
}

pub(crate) struct CardPlayRule<'condition> {
    pub(crate) condition: &'condition Expr<Bool>,
    pub(crate) forbids: bool,
}

pub(crate) fn card_play_rule(ability: &Ability) -> Option<CardPlayRule<'_>> {
    if !matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        || ability.source.holder != (ObjectOperand::SourceHolder {})
        || ability.source.controller != (PlayerRef::Controller {})
    {
        return None;
    }
    let [
        Effect::Permission {
            condition,
            polarity,
            ..
        },
    ] = ability.declarations.as_slice()
    else {
        return None;
    };
    let polarity = match polarity {
        Polarity::Forbid
            if !ability.active_zones.is_empty()
                && ability.active_zones.iter().all(|zone| {
                    matches!(
                        zone,
                        ZoneKind::Hand | ZoneKind::Ex | ZoneKind::Cemetery | ZoneKind::Banish
                    )
                }) =>
        {
            Polarity::Forbid
        }
        Polarity::Allow if ability.active_zones == [ZoneKind::Cemetery] => Polarity::Allow,
        Polarity::Allow | Polarity::Forbid | Polarity::Require => return None,
    };
    let forbids = polarity == Polarity::Forbid;
    let mut expected = declaration(
        ability.source.clone(),
        ability.scope,
        ability.active_zones.clone(),
        condition.clone(),
        polarity,
    );
    expected.id = ability.id;
    (*ability == expected).then_some(CardPlayRule { condition, forbids })
}

fn banish_play_parts(candidate: Binding<ObjectRef>) -> (PlayerSet, Effect, Duration) {
    let players = PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::Implicit,
    };
    let duration = Duration {
        end: DurationEnd::CurrentTurnEnd {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    let objects = Selector::Zone {
        players: players.clone(),
        zone: ZoneKind::Banish,
        view: SelectionView::Current {},
        binding: candidate,
        predicate: Box::new(Expr::Literal(Bool(true))),
    };
    let effect = Effect::Permission {
        subject: PermissionSubject::Players {
            players: players.clone(),
        },
        action: ActionPattern::PlayCard { objects },
        polarity: Polarity::Allow,
        condition: Expr::Literal(Bool(true)),
        duration: duration.clone(),
        rule: RuleId(133),
    };
    (players, effect, duration)
}

pub(crate) fn banish_play_installation(
    affected: Binding<PlayerSet>,
    candidate: Binding<ObjectRef>,
) -> Action {
    let (players, effect, duration) = banish_play_parts(candidate);
    Action::InstallEffect {
        subjects: InstallationSubjects::Players { players, affected },
        capture: InstallationTime::InstructionStart,
        effect: Box::new(effect),
        duration,
    }
}

pub(crate) fn banish_play_filter<'effect>(
    players: &PlayerSet,
    effect: &'effect Effect,
    duration: &Duration,
) -> Option<&'effect Selector> {
    let Effect::Permission {
        action: ActionPattern::PlayCard { objects },
        ..
    } = effect
    else {
        return None;
    };
    let Selector::Zone { binding, .. } = objects else {
        return None;
    };
    let (expected_players, expected_effect, expected_duration) = banish_play_parts(*binding);
    (players == &expected_players && effect == &expected_effect && duration == &expected_duration)
        .then_some(objects)
}

pub(crate) fn is_banish_play_installation(action: &Action) -> bool {
    let Action::InstallEffect {
        subjects: InstallationSubjects::Players { players, .. },
        capture: InstallationTime::InstructionStart,
        effect,
        duration,
    } = action
    else {
        return false;
    };
    banish_play_filter(players, effect, duration).is_some()
}

/// A source-only membership test carries no authority to inspect its hand neighbours.
// Only a predicate-free intersection with SourceHolder avoids enumerating private neighbors.
pub(crate) fn source_zone_query(selector: &Selector) -> Option<ZoneKind> {
    let Selector::Intersection { sets } = selector else {
        return None;
    };
    match sets.as_slice() {
        [
            Selector::Objects { objects },
            Selector::Zone {
                players,
                zone: zone @ (ZoneKind::Hand | ZoneKind::Ex | ZoneKind::Cemetery | ZoneKind::Banish),
                view: SelectionView::Current {},
                predicate,
                ..
            },
        ] if objects == &[ObjectOperand::SourceHolder {}]
            && players.relation == PlayerRelation::SelfPlayer
            && players.relative_to == (PlayerRef::Controller {})
            && **predicate == Expr::Literal(Bool(true)) =>
        {
            Some(*zone)
        }
        _ => None,
    }
}
