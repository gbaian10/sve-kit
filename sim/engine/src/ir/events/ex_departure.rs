//! EX departure subscriptions project the committed batch, independent of its remaining contents.
use super::timing::opponent;
use crate::ir::{
    Binding, Bool, Expr, MoveEvent, PlayerId, PlayerRef, Predicate, ZoneFrozen, ZoneKind,
    ZoneOperation,
};
#[derive(Clone, Copy)]
pub(crate) struct ExDepartureFilter {
    pub(crate) own: bool,
    pub(crate) own_turn: bool,
    pub(crate) to: Option<ZoneKind>,
}
impl ExDepartureFilter {
    pub(crate) fn predicate(self, event: Binding<MoveEvent>) -> Expr<Bool> {
        let player = if self.own {
            PlayerRef::Controller {}
        } else {
            opponent()
        };
        let mut predicates = vec![Expr::Compute(Box::new(Predicate::ZoneEquals {
            left: Expr::Frozen(Box::new(ZoneFrozen::MoveFrom { event })),
            right: Expr::Compute(Box::new(ZoneOperation::OfPlayer {
                player,
                kind: ZoneKind::Ex,
            })),
        }))];
        if let Some(kind) = self.to {
            predicates.push(Expr::Compute(Box::new(Predicate::Or {
                predicates: [PlayerRef::Controller {}, opponent()]
                    .into_iter()
                    .map(|destination_player| {
                        Expr::Compute(Box::new(Predicate::ZoneEquals {
                            left: Expr::Frozen(Box::new(ZoneFrozen::MoveTo { event })),
                            right: Expr::Compute(Box::new(ZoneOperation::OfPlayer {
                                player: destination_player,
                                kind,
                            })),
                        }))
                    })
                    .collect(),
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
    pub(crate) fn parse(event: Binding<MoveEvent>, predicate: &Expr<Bool>) -> Option<Self> {
        for own in [false, true] {
            for own_turn in [false, true] {
                for to in [None, Some(ZoneKind::Cemetery), Some(ZoneKind::Hand)] {
                    let plan = Self { own, own_turn, to };
                    if plan.predicate(event) == *predicate {
                        return Some(plan);
                    }
                }
            }
        }
        None
    }
    pub(crate) fn matches(
        self,
        controller: PlayerId,
        observer: PlayerId,
        active: PlayerId,
    ) -> bool {
        self.own == (controller == observer) && (!self.own_turn || active == observer)
    }
}
