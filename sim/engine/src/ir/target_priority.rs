//! Required targeting is a permission over legal ability-target answers.
use super::{
    ActionPattern, Bool, Cardinality, Duration, DurationEnd, Effect, Expr, Nat, ObjectOperand,
    PermissionSubject, PlayerRef, PlayerRelation, PlayerSet, Polarity, RuleId, Selector,
    TextQuantifier, TimeAnchor,
};

pub(crate) fn requirement() -> Effect {
    Effect::Permission {
        subject: PermissionSubject::Players {
            players: PlayerSet {
                relation: PlayerRelation::Opponents,
                relative_to: PlayerRef::Controller {},
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                text_quantifier: TextQuantifier::Implicit,
            },
        },
        action: ActionPattern::ChooseTarget {
            objects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        },
        polarity: Polarity::Require,
        condition: Expr::Literal(Bool(true)),
        duration: Duration {
            end: DurationEnd::NoTemporalExpiry {},
            anchor: TimeAnchor::Installation {},
            usage: None,
            intervals: vec![],
        },
        rule: RuleId(133),
    }
}

pub(crate) fn is_requirement(effect: &Effect) -> bool {
    *effect == requirement()
}
