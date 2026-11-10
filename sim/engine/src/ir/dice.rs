//! Closed dice predicates and field reroll permissions share one accepted-result model.
use super::{
    Ability, AbilityKind, AbilityOrigin, AbilityQualification, ActionPattern, Binding, Bool,
    Cardinality, Comparison, Control, DiceEvent, DieFace, Duration, DurationEnd, Effect,
    EventPattern, Expr, HistoryCutoff, HistoryQuery, HistoryRange, HistoryWindow, InclusionPolicy,
    IterationCheck, Nat, NatFrozen, ObjectOperand, PermissionSubject, PlayerFrozen, PlayerRef,
    PlayerRelation, PlayerSet, Polarity, Predicate, Program, RepeatBody, RuleId, Termination,
    TextQuantifier, TimeAnchor, ZoneKind,
};

pub(crate) fn predicate(event: Binding<DiceEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::PlayerEquals {
        left: Expr::Frozen(Box::new(PlayerFrozen::DiceRoller { event })),
        right: Expr::Literal(PlayerRef::Controller {}),
    }))
}

pub(crate) const fn permission(times: Nat, rule: RuleId) -> Effect {
    Effect::Permission {
        subject: PermissionSubject::Players {
            players: PlayerSet {
                relative_to: PlayerRef::Controller {},
                relation: PlayerRelation::SelfPlayer,
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
        },
        action: ActionPattern::Reroll {
            player: PlayerRef::Controller {},
            times,
        },
        polarity: Polarity::Allow,
        condition: Expr::Literal(Bool(true)),
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
        rule,
    }
}

pub(crate) fn rerolls(ability: &Ability) -> Option<Nat> {
    let [
        effect @ Effect::Permission {
            action: ActionPattern::Reroll { times, .. },
            rule,
            ..
        },
    ] = ability.declarations.as_slice()
    else {
        return None;
    };
    (ability.kind == AbilityKind::Static
        && ability.qualification == AbilityQualification::Ordinary
        && matches!(ability.source.origin, AbilityOrigin::Printed { .. })
        && ability.source.holder == (ObjectOperand::SourceHolder {})
        && ability.source.controller == (PlayerRef::Controller {})
        && ability.active_zones == [ZoneKind::Field]
        && ability.play.is_none()
        && ability.trigger.is_none()
        && ability.body.is_none()
        && ability.event_captures.is_empty()
        && times.0 > 0
        && *effect == permission(*times, *rule))
    .then_some(*times)
}

pub(crate) fn repeat(control: &Control) -> Option<(&Program, u64)> {
    let Control::Repeat {
        body: RepeatBody::Program { body },
        termination: Termination::Count {
            count: Expr::Literal(count),
        },
        check: IterationCheck::BeforeIteration,
        receipts,
        scope,
    } = control
    else {
        return None;
    };
    (receipts.is_empty() && *scope != body.scope).then_some((body, count.0))
}

pub(crate) fn query(event: Binding<DiceEvent>, face: Option<(Comparison, Nat)>) -> HistoryQuery {
    let own = predicate(event);
    let predicate = match face {
        None => own,
        Some((comparison, value)) => Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                own,
                Expr::Compute(Box::new(Predicate::CompareNat {
                    left: Expr::Frozen(Box::new(NatFrozen::DiceValue { event })),
                    comparison,
                    right: Expr::Literal(value),
                })),
            ],
        })),
    };
    HistoryQuery {
        pattern: EventPattern::Dice {
            binding: event,
            predicate,
        },
        window: HistoryWindow {
            range: HistoryRange::CurrentTurn {},
            cutoff: HistoryCutoff::CurrentPoint {},
            relative_player: None,
        },
        include_execution: InclusionPolicy::IncludeAll {},
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct History {
    pub(crate) face: Option<(Comparison, Nat)>,
}

pub(crate) fn history(query_value: &HistoryQuery) -> Option<History> {
    let EventPattern::Dice { binding, .. } = &query_value.pattern else {
        return None;
    };
    if *query_value == query(*binding, None) {
        return Some(History { face: None });
    }
    for comparison in [
        Comparison::Equal,
        Comparison::LessEqual,
        Comparison::GreaterEqual,
    ] {
        for face in 1..=6 {
            let filter = Some((comparison, Nat(face)));
            if *query_value == query(*binding, filter) {
                return Some(History { face: filter });
            }
        }
    }
    None
}

pub(crate) const fn value(face: DieFace) -> u64 {
    match face {
        DieFace::One => 1,
        DieFace::Two => 2,
        DieFace::Three => 3,
        DieFace::Four => 4,
        DieFace::Five => 5,
        DieFace::Six => 6,
    }
}
