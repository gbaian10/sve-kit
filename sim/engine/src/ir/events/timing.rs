use crate::ir::{
    Ability, AbilityOrigin, DamageEvent, DamageFlags, DamageFlagsFields, DamageRecipient,
    EventPattern, EvolutionState, EvolutionStateFields, KeywordId, LeaveFieldEvent, ObjectOperand,
    RecipientFields, Selector, TriggerCondition,
};
use crate::ir::{
    AttackEvent, Binding, Bool, Cardinality, Edge, EdgeFields, EvolveEvent, Expr, Nat,
    ObjectFrozen, Phase, PhaseEvent, PhaseFields, PlayerFrozen, PlayerRef, PlayerRelation,
    PlayerSet, Predicate, TextQuantifier, ValueEquality,
};

pub(crate) fn opponent() -> PlayerRef {
    PlayerRef::RequireOne {
        players: Box::new(PlayerSet {
            relation: PlayerRelation::Opponents,
            relative_to: PlayerRef::Controller {},
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::One,
        }),
    }
}

pub(crate) fn attack_predicate(event: Binding<AttackEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::ObjectEquals {
        left: Expr::Frozen(Box::new(ObjectFrozen::AttackAttacker { event })),
        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
    }))
}

pub(crate) fn phase_predicate(
    event: Binding<PhaseEvent>,
    phase: Phase,
    player: Option<PlayerRef>,
) -> Expr<Bool> {
    let mut predicates = vec![
        Expr::Compute(Box::new(Predicate::PhaseEquals {
            left: Expr::Frozen(Box::new(PhaseFields::PhasePhase { event })),
            right: Expr::Literal(phase),
        })),
        Expr::Compute(Box::new(Predicate::Equal {
            operands: Box::new(ValueEquality::Edge {
                left: Expr::Frozen(Box::new(EdgeFields::PhaseEdge { event })),
                right: Expr::Literal(Edge::Begin),
            }),
        })),
    ];
    if let Some(player) = player {
        predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
            left: Expr::Frozen(Box::new(PlayerFrozen::PhasePlayer { event })),
            right: Expr::Literal(player),
        })));
    }
    Expr::Compute(Box::new(Predicate::And { predicates }))
}

pub(crate) fn evolve_predicate(event: Binding<EvolveEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::ObjectEquals {
        left: Expr::Frozen(Box::new(ObjectFrozen::EvolveSubject { event })),
        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
    }))
}

pub(crate) fn super_evolve_predicate(event: Binding<EvolveEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            evolve_predicate(event),
            Expr::Compute(Box::new(Predicate::EvolutionEquals {
                left: Expr::Frozen(Box::new(EvolutionStateFields::EvolveKind { event })),
                right: Expr::Literal(EvolutionState::SuperEvolved),
            })),
        ],
    }))
}

fn turn_predicate(own: bool) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::PlayerEquals {
        left: Expr::Literal(PlayerRef::Active {}),
        right: Expr::Literal(if own {
            PlayerRef::Controller {}
        } else {
            opponent()
        }),
    }))
}

pub(crate) fn leave_predicate(event: Binding<LeaveFieldEvent>, own_turn: bool) -> Expr<Bool> {
    let mut predicates = vec![Expr::Compute(Box::new(Predicate::ObjectEquals {
        left: Expr::Frozen(Box::new(ObjectFrozen::LeaveFieldObjectBefore { event })),
        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
    }))];
    if own_turn {
        predicates.push(turn_predicate(true));
    }
    Expr::Compute(Box::new(Predicate::And { predicates }))
}

pub(crate) fn leave_filter(
    event: Binding<LeaveFieldEvent>,
    predicate: &Expr<Bool>,
) -> Option<bool> {
    [false, true]
        .into_iter()
        .find(|own| *predicate == leave_predicate(event, *own))
}

#[derive(Clone, Copy)]
pub(crate) struct DamageFilter {
    pub(crate) ability: bool,
    pub(crate) own_turn: Option<bool>,
}
impl DamageFilter {
    pub(crate) fn predicate(self, event: Binding<DamageEvent>) -> Expr<Bool> {
        let mut predicates = vec![Expr::Compute(Box::new(Predicate::RecipientIsFollower {
            recipient: DamageRecipient::FromExpression {
                value: Box::new(Expr::Frozen(Box::new(RecipientFields::DamageRecipient {
                    event,
                }))),
            },
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        }))];
        if self.ability {
            predicates.push(Expr::Compute(Box::new(Predicate::Equal {
                operands: Box::new(ValueEquality::DamageFlags {
                    left: Expr::Frozen(Box::new(DamageFlagsFields::DamageFlags { event })),
                    right: Expr::Literal(DamageFlags::Ability),
                }),
            })));
        }
        if let Some(own) = self.own_turn {
            predicates.push(turn_predicate(own));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }
    pub(crate) fn parse(event: Binding<DamageEvent>, predicate: &Expr<Bool>) -> Option<Self> {
        [false, true]
            .into_iter()
            .flat_map(|ability| {
                [None, Some(false), Some(true)]
                    .into_iter()
                    .map(move |own_turn| Self { ability, own_turn })
            })
            .find(|filter| *predicate == filter.predicate(event))
    }
}

pub(crate) fn second_header(ability: &Ability) -> Option<bool> {
    let super_keyword = matches!(
        ability.source.origin,
        AbilityOrigin::Keyword {
            keyword: KeywordId::OnSuperEvolve,
            ..
        }
    );
    let Some(trigger) = ability.trigger.as_ref() else {
        return super_keyword.then_some(false);
    };
    let TriggerCondition::Event { pattern } = &trigger.on else {
        return super_keyword.then_some(false);
    };
    match (&ability.source.origin, pattern) {
        (
            AbilityOrigin::Keyword {
                keyword: KeywordId::OnSuperEvolve,
                origin,
            },
            EventPattern::Evolve { binding, predicate },
        ) => Some(
            matches!(origin.as_ref(), AbilityOrigin::Printed { .. })
                && *predicate == super_evolve_predicate(*binding),
        ),
        (
            AbilityOrigin::Keyword {
                keyword: KeywordId::OnSuperEvolve,
                ..
            },
            _,
        ) => Some(false),
        (AbilityOrigin::Printed { .. }, EventPattern::LeaveField { binding, predicate }) => {
            Some(leave_filter(*binding, predicate).is_some())
        }
        (AbilityOrigin::Printed { .. }, EventPattern::Damage { binding, predicate }) => {
            Some(DamageFilter::parse(*binding, predicate).is_some())
        }
        (AbilityOrigin::Printed { .. }, EventPattern::Phase { binding, predicate }) => {
            Some([Phase::Main, Phase::End].into_iter().any(|phase| {
                [None, Some(PlayerRef::Controller {}), Some(opponent())]
                    .into_iter()
                    .any(|player| *predicate == phase_predicate(*binding, phase, player))
            }))
        }
        _ => None,
    }
}
