//! Entry subscriptions compile into a closed predicate plan before any authority mutation.
use crate::ir::Characteristics;
use crate::ir::{
    Binding, Bool, CardKind, CharacteristicsFields, EnterFieldEvent, Expr, KindFrozen,
    ObjectFrozen, PlayerFrozen, PlayerRef, Predicate, TraitId, TraitsFrozen,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct EntryFilter {
    pub(crate) own: Option<bool>,
    pub(crate) itself: Option<bool>,
    pub(crate) follower: bool,
    pub(crate) trait_id: Option<TraitId>,
    pub(crate) own_turn: bool,
}

fn object_test(event: Binding<EnterFieldEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::ObjectEquals {
        left: Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject { event })),
        right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
    }))
}
fn characters(event: Binding<EnterFieldEvent>) -> Expr<Characteristics> {
    Expr::Frozen(Box::new(CharacteristicsFields::EnterFieldCharacters {
        event,
    }))
}

impl EntryFilter {
    pub(crate) fn predicate(&self, event: Binding<EnterFieldEvent>) -> Expr<Bool> {
        let mut predicates = Vec::new();
        if let Some(positive) = self.itself {
            let predicate = object_test(event);
            predicates.push(if positive {
                predicate
            } else {
                Expr::Compute(Box::new(Predicate::Not { predicate }))
            });
        }
        if let Some(own) = self.own {
            predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Frozen(Box::new(PlayerFrozen::EnterFieldController { event })),
                right: Expr::Literal(if own {
                    PlayerRef::Controller {}
                } else {
                    super::timing::opponent()
                }),
            })));
        }
        if self.follower {
            predicates.push(Expr::Compute(Box::new(Predicate::KindEquals {
                left: Expr::Frozen(Box::new(KindFrozen::Characters {
                    characters: characters(event),
                })),
                right: Expr::Literal(CardKind::Follower),
            })));
        }
        if let Some(trait_id) = &self.trait_id {
            predicates.push(Expr::Compute(Box::new(Predicate::HasTrait {
                traits: Expr::Frozen(Box::new(TraitsFrozen::Characters {
                    characters: characters(event),
                })),
                trait_id: trait_id.clone(),
            })));
        }
        if self.own_turn {
            predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Literal(PlayerRef::Active {}),
                right: Expr::Literal(PlayerRef::Controller {}),
            })));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }

    pub(crate) fn parse(event: Binding<EnterFieldEvent>, expression: &Expr<Bool>) -> Option<Self> {
        let mut plan = Self::default();
        plan.extend(event, expression)?;
        if plan.itself != Some(true) && !(plan.follower && plan.own.is_some()) {
            return None;
        }
        if *expression != plan.predicate(event) && *expression != object_test(event) {
            return None;
        }
        Some(plan)
    }

    fn extend(&mut self, event: Binding<EnterFieldEvent>, expression: &Expr<Bool>) -> Option<()> {
        let Expr::Compute(predicate) = expression else {
            return None;
        };
        if let Predicate::And { predicates } = predicate.as_ref() {
            for part in predicates {
                self.extend(event, part)?;
            }
            return Some(());
        }
        let itself = object_test(event);
        if *expression == itself {
            self.itself = Some(true);
        } else if matches!(predicate.as_ref(), Predicate::Not { predicate } if *predicate == itself)
        {
            self.itself = Some(false);
        } else if matches!(predicate.as_ref(), Predicate::PlayerEquals { left, right }
            if *left == Expr::Frozen(Box::new(PlayerFrozen::EnterFieldController { event }))
            && *right == Expr::Literal(PlayerRef::Controller {}))
        {
            self.own = Some(true);
        } else if matches!(predicate.as_ref(), Predicate::PlayerEquals { left, right }
            if *left == Expr::Frozen(Box::new(PlayerFrozen::EnterFieldController { event }))
            && *right == Expr::Literal(super::timing::opponent()))
        {
            self.own = Some(false);
        } else if matches!(predicate.as_ref(), Predicate::KindEquals { left, right }
            if *left == Expr::Frozen(Box::new(KindFrozen::Characters { characters: characters(event) }))
            && *right == Expr::Literal(CardKind::Follower))
        {
            self.follower = true;
        } else if let Predicate::HasTrait { traits, trait_id } = predicate.as_ref() {
            if *traits
                != Expr::Frozen(Box::new(TraitsFrozen::Characters {
                    characters: characters(event),
                }))
                || self.trait_id.is_some()
            {
                return None;
            }
            self.trait_id = Some(trait_id.clone());
        } else if matches!(predicate.as_ref(), Predicate::PlayerEquals { left, right }
            if *left == Expr::Literal(PlayerRef::Active {}) && *right == Expr::Literal(PlayerRef::Controller {}))
        {
            self.own_turn = true;
        } else {
            return None;
        }
        Some(())
    }
}
