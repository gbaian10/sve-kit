use super::{
    Action, Allocator, Binding, Cardinality, LoweringError, Nat, PlayerRef, PlayerRelation,
    PlayerSet, Selector, TextQuantifier, ZoneKind, ast, unsupported,
};
use crate::ir::{
    CardKind, ChoiceDomain, Decision, DecisionId, DecisionKind, Disjointness, ExecutionRef,
    InputMode, PlaySlot, SelectionConstraint, SelectionGroup, SelectionSnapshot, SelectionView,
    Visibility,
};

pub(super) fn lower(
    allocator: &mut Allocator,
    selected: Binding<Selector>,
    material: &ast::FlipMaterial,
    up: bool,
    path: &str,
) -> Result<(Action, Decision), LoweringError> {
    if material.n == 0
        || material.result.is_some()
        || !matches!(material.sel.kind, ast::SearchKind::EvolveFollower)
        || material.sel.face_up.is_some_and(|before| before == up)
    {
        return Err(unsupported(path));
    }
    let candidate = Binding::new(1, allocator.scope(path)?);
    let players = PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::Implicit,
    };
    let bounds = Cardinality {
        minimum: Nat(material.n),
        maximum: Some(Nat(material.n)),
    };
    let selector = Selector::Zone {
        players: players.clone(),
        zone: ZoneKind::Evolve,
        view: SelectionView::Current {},
        binding: candidate,
        predicate: Box::new(super::selection::field_filter(
            &super::selection::ObjectFilter {
                kind: Some(CardKind::EvolveFollower),
                face_up: Some(!up),
                name: material.sel.name.as_deref(),
                contains: material.sel.name_contains.as_deref(),
                trait_id: material.sel.r#trait.as_deref(),
                ..Default::default()
            },
            candidate,
        )),
    };
    let objects = Selector::Bound {
        reference: selected,
    };
    let action = if up {
        Action::FlipUp {
            objects,
            faces: vec![],
        }
    } else {
        Action::FlipDown { objects }
    };
    Ok((
        action,
        Decision {
            id: DecisionId(allocator.id(path)?),
            captures: vec![],
            actor: PlayerRef::Controller {},
            visibility: if up {
                Visibility::Only { players }
            } else {
                Visibility::Public {}
            },
            kind: DecisionKind::PlayParameter {
                slot: PlaySlot::CostMaterials,
                selection_snapshot: SelectionSnapshot::AtDecision,
                play_scope: ExecutionRef::Current {},
                input: InputMode::Inline,
            },
            domain: ChoiceDomain::Objects {
                binding: selected,
                target_roles: None,
                constraint: SelectionConstraint {
                    groups: vec![SelectionGroup {
                        selector,
                        cardinality: bounds,
                    }],
                    cardinality: bounds,
                    distinct_by: None,
                    disjoint: Disjointness::Disjoint,
                    aggregate_bounds: vec![],
                },
            },
        },
    ))
}
