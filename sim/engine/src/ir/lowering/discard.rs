use super::super::{
    DiscardMode, Disjointness, SelectionConstraint, SelectionGroup, SelectionView, ZoneKind,
};
use super::{
    Action, Binding, BodyContext, Bool, Cardinality, Expr, Int, LoweringError, Nat, PlayerRef,
    PlayerRelation, PlayerSet, Selector, TextQuantifier, ast, unsupported,
};
use crate::ir::events::discard::DiscardFilter;

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Discard shares the body's binding allocator."
)]
impl BodyContext<'_> {
    pub(super) fn discard(
        &mut self,
        value: &ast::Discard,
        path: &str,
    ) -> Result<Action, LoweringError> {
        let player = match value.who {
            Some(ast::LeaderSide::Opponent) => super::super::events::timing::opponent(),
            Some(ast::LeaderSide::Controller) | None => PlayerRef::Controller {},
        };
        let mode = match (value.n, value.all) {
            (Some(n), None) if n > 0 && value.random != Some(true) => {
                let cardinality = Cardinality {
                    minimum: Nat(n),
                    maximum: Some(Nat(n)),
                };
                DiscardMode::Chosen {
                    constraint: SelectionConstraint {
                        groups: vec![SelectionGroup {
                            selector: Selector::Zone {
                                players: PlayerSet {
                                    relation: PlayerRelation::SelfPlayer,
                                    relative_to: player.clone(),
                                    cardinality: Cardinality {
                                        minimum: Nat(1),
                                        maximum: Some(Nat(1)),
                                    },
                                    text_quantifier: TextQuantifier::Implicit,
                                },
                                zone: ZoneKind::Hand,
                                view: SelectionView::Current {},
                                binding: Binding::new(self.allocator.id(path)?, self.scope),
                                predicate: Box::new(Expr::Literal(Bool(true))),
                            },
                            cardinality,
                        }],
                        cardinality,
                        distinct_by: None,
                        disjoint: Disjointness::Disjoint,
                        aggregate_bounds: vec![],
                    },
                }
            }
            (Some(n), None) if n > 0 && value.random == Some(true) => DiscardMode::Random {
                population: Selector::Zone {
                    players: PlayerSet {
                        relation: PlayerRelation::SelfPlayer,
                        relative_to: player.clone(),
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        text_quantifier: TextQuantifier::Implicit,
                    },
                    zone: ZoneKind::Hand,
                    view: SelectionView::Current {},
                    binding: Binding::new(self.allocator.id(path)?, self.scope),
                    predicate: Box::new(Expr::Literal(Bool(true))),
                },
                count: Expr::Literal(Int(i64::try_from(n).map_err(|_overflow| unsupported(path))?)),
            },
            (None, Some(true)) if value.random.is_none() => DiscardMode::All {
                eligible: Selector::Zone {
                    players: PlayerSet {
                        relation: PlayerRelation::SelfPlayer,
                        relative_to: player.clone(),
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        text_quantifier: TextQuantifier::Implicit,
                    },
                    zone: ZoneKind::Hand,
                    view: SelectionView::Current {},
                    binding: Binding::new(self.allocator.id(path)?, self.scope),
                    predicate: Box::new(Expr::Literal(Bool(true))),
                },
            },
            _ => return Err(unsupported(path)),
        };
        Ok(Action::Discard { player, mode })
    }
}

pub(super) fn subscription(
    discard: &ast::DiscardSubscription,
    event_id: u64,
    scope: super::ScopeId,
    path: &str,
) -> Result<super::EventPattern, LoweringError> {
    if discard.reference.is_some() && matches!(discard.who, Some(ast::EntryPlayer::Opponent)) {
        return Err(unsupported(path));
    }
    let event = Binding::new(event_id, scope);
    Ok(super::EventPattern::Discard {
        binding: event,
        predicate: DiscardFilter {
            own: discard
                .who
                .as_ref()
                .map(|who| matches!(who, ast::EntryPlayer::Controller)),
            itself: discard.reference.is_some(),
        }
        .predicate(event),
    })
}
