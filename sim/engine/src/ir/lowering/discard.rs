use super::super::{
    DiscardMode, Disjointness, SelectionConstraint, SelectionGroup, SelectionView, ZoneKind,
};
use super::{
    Action, Binding, BodyContext, Bool, Cardinality, Expr, LoweringError, Nat, PlayerRef,
    PlayerRelation, PlayerSet, Selector, TextQuantifier, ast, unsupported,
};

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
        if value.n == 0 || value.random == Some(true) {
            return Err(unsupported(path));
        }
        let cardinality = Cardinality {
            minimum: Nat(value.n),
            maximum: Some(Nat(value.n)),
        };
        Ok(Action::Discard {
            player: PlayerRef::Controller {},
            mode: DiscardMode::Chosen {
                constraint: SelectionConstraint {
                    groups: vec![SelectionGroup {
                        selector: Selector::Zone {
                            players: PlayerSet {
                                relation: PlayerRelation::SelfPlayer,
                                relative_to: PlayerRef::Controller {},
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
            },
        })
    }
}
