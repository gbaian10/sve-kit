//! Deck-to-cemetery headers read only the committed move's zones and the active player.
use super::timing::opponent;
use crate::ir::{
    Binding, Bool, Expr, MoveEvent, PlayerId, PlayerRef, Predicate, ZoneFrozen, ZoneKind,
    ZoneOperation, ZoneRef,
};

/// 「（相手の）デッキのカードが墓場に置かれたとき」 for one deck owner.
#[derive(Clone, Copy)]
pub(crate) struct MillFilter {
    pub(crate) own: bool,
    pub(crate) own_turn: bool,
}

impl MillFilter {
    fn zone(self, kind: ZoneKind) -> Expr<ZoneRef> {
        Expr::Compute(Box::new(ZoneOperation::OfPlayer {
            player: if self.own {
                PlayerRef::Controller {}
            } else {
                opponent()
            },
            kind,
        }))
    }

    pub(crate) fn predicate(self, event: Binding<MoveEvent>) -> Expr<Bool> {
        let mut predicates = vec![
            Expr::Compute(Box::new(Predicate::ZoneEquals {
                left: Expr::Frozen(Box::new(ZoneFrozen::MoveFrom { event })),
                right: self.zone(ZoneKind::Deck),
            })),
            Expr::Compute(Box::new(Predicate::ZoneEquals {
                left: Expr::Frozen(Box::new(ZoneFrozen::MoveTo { event })),
                right: self.zone(ZoneKind::Cemetery),
            })),
        ];
        if self.own_turn {
            predicates.push(Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Literal(PlayerRef::Active {}),
                right: Expr::Literal(PlayerRef::Controller {}),
            })));
        }
        Expr::Compute(Box::new(Predicate::And { predicates }))
    }

    pub(crate) fn parse(event: Binding<MoveEvent>, expression: &Expr<Bool>) -> Option<Self> {
        [false, true]
            .into_iter()
            .flat_map(|own| [false, true].map(|own_turn| Self { own, own_turn }))
            .find(|plan| plan.predicate(event) == *expression)
    }

    /// The card stays with its owner, so the deck owner is also the cemetery owner.
    pub(crate) fn matches(self, owner: PlayerId, observer: PlayerId, active: PlayerId) -> bool {
        self.own == (owner == observer) && (!self.own_turn || active == observer)
    }
}
