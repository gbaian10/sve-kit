//! Both sides of an actual damage fact share the ordinary timing subscription.
use super::subject::{SubjectFields, SubjectFilter};
use super::timing::turn_predicate;
use crate::ir::{
    Binding, Bool, CardKind, Cardinality, CharacteristicView, DamageEvent, DamageFlags,
    DamageFlagsFields, DamageRecipient, Expr, KindRead, Nat, ObjectOperand, ObjectRef, ObjectView,
    PlayerRef, PlayerRelation, PlayerSet, Predicate, RecipientFields, SelectionView, Selector,
    TextQuantifier, ValueEquality, ZoneKind,
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
#[expect(
    variant_size_differences,
    reason = "The field candidate keeps its small binding inline without allocating per subscription."
)]
pub(crate) enum RecipientFilter {
    SourceHolder,
    Leader(Option<bool>),
    OpposingFollowers { candidate: Binding<ObjectRef> },
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
            Self::OpposingFollowers { candidate } => Predicate::RecipientIsFollower {
                recipient,
                objects: Selector::Zone {
                    players: PlayerSet {
                        relation: PlayerRelation::Opponents,
                        relative_to: PlayerRef::Controller {},
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        text_quantifier: TextQuantifier::Implicit,
                    },
                    zone: ZoneKind::Field,
                    view: SelectionView::Current {},
                    binding: candidate,
                    predicate: Box::new(Expr::Compute(Box::new(Predicate::KindEquals {
                        left: Expr::Live(Box::new(KindRead::Kind {
                            object: ObjectView::Current {
                                object: Box::new(ObjectOperand::Bound { binding: candidate }),
                            },
                            view: CharacteristicView::Effective,
                        })),
                        right: Expr::Literal(CardKind::Follower),
                    }))),
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
    pub(crate) const fn requires_instruction_batch(&self) -> bool {
        matches!(
            self.recipient,
            Some(RecipientFilter::OpposingFollowers { .. })
        )
    }

    pub(crate) fn instruction_batch(&self, event: Binding<DamageEvent>) -> bool {
        self.requires_instruction_batch()
            && self.predicate(event)
                == Self {
                    dealt: Some(SubjectFilter {
                        itself: Some(true),
                        ..SubjectFilter::default()
                    }),
                    recipient: self.recipient,
                    kind: Some(DamageKind::Ability),
                    own_turn: None,
                }
                .predicate(event)
    }
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
        let candidate = if let Expr::Compute(root) = expression
            && let Predicate::And { predicates } = root.as_ref()
        {
            predicates.iter().find_map(|part| {
                let Expr::Compute(operation) = part else {
                    return None;
                };
                let Predicate::RecipientIsFollower {
                    objects: Selector::Zone { binding, .. },
                    ..
                } = operation.as_ref()
                else {
                    return None;
                };
                Some(*binding)
            })
        } else {
            None
        };
        for recipient in [
            None,
            Some(RecipientFilter::SourceHolder),
            Some(RecipientFilter::Leader(None)),
            Some(RecipientFilter::Leader(Some(false))),
            Some(RecipientFilter::Leader(Some(true))),
            candidate.map(|candidate| RecipientFilter::OpposingFollowers { candidate }),
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
