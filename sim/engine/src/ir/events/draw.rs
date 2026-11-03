//! Draw headers inspect the committed draw and its occurrence phase, never a hand movement.
use crate::ir::{
    Binding, Bool, DrawEvent, EventContext, Expr, Phase, PhaseFields, PlayerFrozen, PlayerRef,
    Predicate,
};

pub(crate) fn predicate(event: Binding<DrawEvent>) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            Expr::Compute(Box::new(Predicate::PlayerEquals {
                left: Expr::Frozen(Box::new(PlayerFrozen::DrawPlayer { event })),
                right: Expr::Literal(PlayerRef::Controller {}),
            })),
            Expr::Compute(Box::new(Predicate::Not {
                predicate: Expr::Compute(Box::new(Predicate::PhaseEquals {
                    left: Expr::Frozen(Box::new(PhaseFields::HeaderPhase {
                        event: EventContext::Draw { event },
                    })),
                    right: Expr::Literal(Phase::Start),
                })),
            })),
        ],
    }))
}
