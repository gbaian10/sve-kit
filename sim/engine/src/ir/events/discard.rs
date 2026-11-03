//! Discard headers use the committed hand action and its original subject identity.
use super::timing::{opponent, turn_predicate};
use crate::ir::{
    Binding, Bool, DiscardEvent, Expr, ObjectFrozen, ObjectRef, PlayerFrozen, PlayerId, PlayerRef,
    Predicate,
};

#[derive(Clone, Copy)]
pub(crate) struct DiscardFilter {
    pub(crate) own: Option<bool>,
    pub(crate) itself: bool,
    pub(crate) own_turn: bool,
}
impl DiscardFilter {
    pub(crate) fn predicate(self, event: Binding<DiscardEvent>) -> Expr<Bool> {
        let mut predicates = Vec::new();
        if let Some(own) = self.own {
            predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Frozen(Box::new(PlayerFrozen::DiscardPlayer { event })),
                right: Expr::Literal(if own {
                    PlayerRef::Controller {}
                } else {
                    opponent()
                }),
            })));
        }
        if self.itself {
            predicates.push(Expr::Compute(Box::new(Predicate::ObjectEquals {
                left: Expr::Frozen(Box::new(ObjectFrozen::DiscardCardBefore { event })),
                right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
            })));
        }
        if self.own_turn {
            predicates.push(turn_predicate(true));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }
    pub(crate) fn parse(event: Binding<DiscardEvent>, expression: &Expr<Bool>) -> Option<Self> {
        [None, Some(false), Some(true)]
            .into_iter()
            .flat_map(|own| {
                [false, true].into_iter().flat_map(move |itself| {
                    [false, true].map(move |own_turn| Self {
                        own,
                        itself,
                        own_turn,
                    })
                })
            })
            .find(|plan| plan.predicate(event) == *expression)
    }
    pub(crate) fn matches(
        self,
        player: PlayerId,
        card: ObjectRef,
        source: ObjectRef,
        controller: PlayerId,
        active: PlayerId,
    ) -> bool {
        self.own.is_none_or(|own| own == (player == controller))
            && (!self.itself || card == source)
            && (!self.own_turn || active == controller)
    }
}
