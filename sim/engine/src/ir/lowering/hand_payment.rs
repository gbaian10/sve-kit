use super::{
    Allocator, Binding, Cardinality, LoweringError, Nat, PlayerRef, PlayerRelation, PlayerSet,
    TextQuantifier, ZoneKind, ast, search, unsupported,
};
use crate::ir::{
    Action, DiscardMode, Disjointness, SelectionConstraint, SelectionGroup, SelectionView, Selector,
};

pub(super) fn lower(
    allocator: &mut Allocator,
    material: &ast::HandMaterial,
    path: &str,
) -> Result<Action, LoweringError> {
    if material.n == 0 {
        return Err(unsupported(path));
    }
    let binding = Binding::new(allocator.id(path)?, allocator.scope(path)?);
    let cardinality = Cardinality {
        minimum: Nat(material.n),
        maximum: Some(Nat(material.n)),
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
                        binding,
                        predicate: Box::new(search::object_filter(
                            material.sel.kind.as_ref(),
                            material.sel.r#trait.as_deref(),
                            material.sel.name.as_deref(),
                            binding,
                        )),
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
