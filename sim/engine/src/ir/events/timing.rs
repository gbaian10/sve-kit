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

pub(crate) fn end_predicate(event: Binding<PhaseEvent>, player: Option<PlayerRef>) -> Expr<Bool> {
    let mut predicates = vec![
        Expr::Compute(Box::new(Predicate::PhaseEquals {
            left: Expr::Frozen(Box::new(PhaseFields::PhasePhase { event })),
            right: Expr::Literal(Phase::End),
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
