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
    if material.n == 0 || material.result.is_some() {
        return Err(unsupported(path));
    }
    let bounds = Cardinality {
        minimum: Nat(material.n),
        maximum: Some(Nat(material.n)),
    };
    let action = if up {
        Action::FlipUp {
            objects: Selector::Bound {
                reference: selected,
            },
            faces: vec![],
        }
    } else {
        Action::FlipDown {
            objects: Selector::Bound {
                reference: selected,
            },
        }
    };
    let selection = FlipSelection {
        filter: &material.sel,
        bounds,
        before: !up,
        kind: DecisionKind::PlayParameter {
            slot: PlaySlot::CostMaterials,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
    };
    Ok((action, selection.lower(allocator, selected, path)?))
}

pub(super) struct FlipSelection<'filter> {
    pub filter: &'filter ast::FlipFilter,
    pub bounds: Cardinality,
    pub before: bool,
    pub kind: DecisionKind,
}

impl FlipSelection<'_> {
    pub(super) fn lower(
        self,
        allocator: &mut Allocator,
        selected: Binding<Selector>,
        path: &str,
    ) -> Result<Decision, LoweringError> {
        let Self {
            filter,
            bounds,
            before,
            kind,
        } = self;
        if !matches!(filter.kind, ast::SearchKind::EvolveFollower)
            || filter.face_up.is_some_and(|face_up| face_up != before)
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
        let selector = Selector::Zone {
            players: players.clone(),
            zone: ZoneKind::Evolve,
            view: SelectionView::Current {},
            binding: candidate,
            predicate: Box::new(super::selection::field_filter(
                &super::selection::ObjectFilter {
                    kind: Some(CardKind::EvolveFollower),
                    face_up: Some(before),
                    name: filter.name.as_deref(),
                    contains: filter.name_contains.as_deref(),
                    trait_id: filter.r#trait.as_deref(),
                    ..Default::default()
                },
                candidate,
            )),
        };
        Ok(Decision {
            id: DecisionId(allocator.id(path)?),
            captures: vec![],
            actor: PlayerRef::Controller {},
            visibility: if before {
                Visibility::Public {}
            } else {
                Visibility::Only { players }
            },
            kind,
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
        })
    }
}
