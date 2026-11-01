//! Both sides of an actual damage fact share the ordinary timing subscription.
use super::subject::{SubjectFields, SubjectFilter};
use super::timing::turn_predicate;
use crate::ir::{
    Binding, Bool, Cardinality, DamageEvent, DamageFlags, DamageFlagsFields, DamageRecipient, Expr,
    Nat, ObjectOperand, PlayerRef, PlayerRelation, PlayerSet, Predicate, RecipientFields, Selector,
    TextQuantifier, ValueEquality,
};

#[derive(Clone, Copy)]
pub(crate) enum DamageKind {
    Attack,
    Combat,
    Ability,
}
impl DamageKind {
    pub(crate) const fn matches(self, flags: DamageFlags) -> bool {
        match self {
            Self::Attack => matches!(flags, DamageFlags::Attack | DamageFlags::AttackCombat),
            Self::Combat => matches!(flags, DamageFlags::Combat | DamageFlags::AttackCombat),
            Self::Ability => matches!(flags, DamageFlags::Ability),
        }
    }
    fn predicate(self, event: Binding<DamageEvent>) -> Expr<Bool> {
        let flag = |flags| {
            Expr::Compute(Box::new(Predicate::Equal {
                operands: Box::new(ValueEquality::DamageFlags {
                    left: Expr::Frozen(Box::new(DamageFlagsFields::DamageFlags { event })),
                    right: Expr::Literal(flags),
                }),
            }))
        };
        match self {
            Self::Attack => Expr::Compute(Box::new(Predicate::Or {
                predicates: vec![flag(DamageFlags::Attack), flag(DamageFlags::AttackCombat)],
            })),
            Self::Ability => flag(DamageFlags::Ability),
            Self::Combat => Expr::Compute(Box::new(Predicate::Or {
                predicates: vec![flag(DamageFlags::Combat), flag(DamageFlags::AttackCombat)],
            })),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RecipientFilter {
    SourceHolder,
    Leader(Option<bool>),
}
impl RecipientFilter {
    fn predicate(self, event: Binding<DamageEvent>) -> Expr<Bool> {
        let recipient = DamageRecipient::FromExpression {
            value: Box::new(Expr::Frozen(Box::new(RecipientFields::DamageRecipient {
                event,
            }))),
        };
        Expr::Compute(Box::new(match self {
            Self::SourceHolder => Predicate::RecipientIsFollower {
                recipient,
                objects: Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
            },
            Self::Leader(own) => Predicate::RecipientIsLeader {
                recipient,
                players: PlayerSet {
                    relation: match own {
                        Some(true) => PlayerRelation::SelfPlayer,
                        Some(false) => PlayerRelation::Opponents,
                        None => PlayerRelation::All,
                    },
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(u64::from(own.is_some())),
                        maximum: own.map(|_| Nat(1)),
                    },
                    text_quantifier: if own.is_some() {
                        TextQuantifier::One
                    } else {
                        TextQuantifier::All
                    },
                },
            },
        }))
    }
}

#[derive(Clone)]
pub(crate) struct DamageFilter {
    pub(crate) dealt: Option<SubjectFilter>,
    pub(crate) recipient: Option<RecipientFilter>,
    pub(crate) kind: Option<DamageKind>,
    pub(crate) own_turn: Option<bool>,
}
impl DamageFilter {
    pub(crate) fn predicate(&self, event: Binding<DamageEvent>) -> Expr<Bool> {
        let mut predicates = Vec::new();
        if let Some(subject) = &self.dealt {
            predicates.push(subject.predicate(SubjectFields::DamageSource(event)));
        }
        if let Some(recipient) = self.recipient {
            predicates.push(recipient.predicate(event));
        }
        if let Some(kind) = self.kind {
            predicates.push(kind.predicate(event));
        }
        if let Some(own) = self.own_turn {
            predicates.push(turn_predicate(own));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }
    pub(crate) fn parse(event: Binding<DamageEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let dealt = if let Expr::Compute(root) = expression {
            if let Predicate::And { predicates } = root.as_ref() {
                predicates
                    .first()
                    .and_then(|part| SubjectFilter::parse(SubjectFields::DamageSource(event), part))
            } else {
                None
            }
        } else {
            None
        };
        for recipient in [
            None,
            Some(RecipientFilter::SourceHolder),
            Some(RecipientFilter::Leader(None)),
            Some(RecipientFilter::Leader(Some(false))),
            Some(RecipientFilter::Leader(Some(true))),
        ] {
            for kind in [
                None,
                Some(DamageKind::Attack),
                Some(DamageKind::Combat),
                Some(DamageKind::Ability),
            ] {
                for own_turn in [None, Some(false), Some(true)] {
                    let filter = Self {
                        dealt: dealt.clone(),
                        recipient,
                        kind,
                        own_turn,
                    };
                    if filter.predicate(event) == *expression {
                        return Some(filter);
                    }
                }
            }
        }
        None
    }
}
