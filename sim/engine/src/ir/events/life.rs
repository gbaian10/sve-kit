//! Leader life subscriptions share the ordinary per-fact timing admission.
use super::timing::{opponent, turn_predicate};
use crate::ir::{
    Binding, Bool, Comparison, Expr, Int, IntFrozen, LifeChangeEvent, PlayerFrozen, PlayerId,
    PlayerRef, Predicate,
};

#[derive(Clone, Copy)]
pub(crate) struct LifeFilter {
    pub(crate) own: Option<bool>,
    pub(crate) up: Option<bool>,
    pub(crate) own_turn: Option<bool>,
}
impl LifeFilter {
    pub(crate) fn predicate(self, event: Binding<LifeChangeEvent>) -> Expr<Bool> {
        let mut predicates = Vec::new();
        if let Some(own) = self.own {
            predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Frozen(Box::new(PlayerFrozen::LifeChangeSubject { event })),
                right: Expr::Literal(if own {
                    PlayerRef::Controller {}
                } else {
                    opponent()
                }),
            })));
        }
        if let Some(up) = self.up {
            predicates.push(Expr::Compute(Box::new(Predicate::CompareInt {
                left: Expr::Frozen(Box::new(IntFrozen::LifeChangeDelta { event })),
                comparison: if up {
                    Comparison::Greater
                } else {
                    Comparison::Less
                },
                right: Expr::Literal(Int(0)),
            })));
        }
        if let Some(own) = self.own_turn {
            predicates.push(turn_predicate(own));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }
    pub(crate) fn parse(event: Binding<LifeChangeEvent>, expression: &Expr<Bool>) -> Option<Self> {
        [None, Some(false), Some(true)]
            .into_iter()
            .flat_map(|own| {
                [None, Some(false), Some(true)]
                    .into_iter()
                    .flat_map(move |up| {
                        [None, Some(false), Some(true)]
                            .into_iter()
                            .map(move |own_turn| Self { own, up, own_turn })
                    })
            })
            .find(|plan| plan.predicate(event) == *expression)
    }
    pub(crate) fn matches(
        self,
        event: LifeChangeEvent,
        controller: PlayerId,
        active: PlayerId,
    ) -> bool {
        event.delta.0 != 0
            && self
                .own
                .is_none_or(|own| own == (controller == event.subject))
            && self.up.is_none_or(|up| up == (event.delta.0 > 0))
            && self
                .own_turn
                .is_none_or(|own| own == (controller == active))
    }
}
