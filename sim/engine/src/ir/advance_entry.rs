//! One private named Advance choice can authorize only its adjacent field movement.
use super::{
    AbilitySource, Action, Binding, Bool, BoolRead, BooleanState, CardName, Cardinality,
    ChoiceDomain, Control, Decision, DecisionId, DecisionKind, Declinable, Disjointness, Expr,
    Instruction, InstructionContext, InstructionId, Located, MoveDestination, MoveReason, MoveSpec,
    NamesRead, Nat, Node, ObjectOperand, ObjectRef, ObjectView, PlayerRef, PlayerRelation,
    PlayerSet, Predicate, Program, ReceiptId, ScopeId, SelectionConstraint, SelectionGroup,
    SelectionView, Selector, SourceSpan, TextQuantifier, Visibility, ZoneKind, ZoneRef,
};

pub(crate) struct Keys {
    pub scope: ScopeId,
    pub candidate: ScopeId,
    pub decision: DecisionId,
    pub instruction: InstructionId,
    pub receipt: ReceiptId,
}

const fn bounds() -> Cardinality {
    Cardinality {
        minimum: Nat(0),
        maximum: Some(Nat(1)),
    }
}

const fn controller() -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: PlayerRef::Controller {},
        cardinality: Cardinality {
            minimum: Nat(1),
            maximum: Some(Nat(1)),
        },
        text_quantifier: TextQuantifier::Implicit,
    }
}

fn predicate(candidate: Binding<ObjectRef>, name: &CardName) -> Expr<Bool> {
    let object = ObjectView::Current {
        object: Box::new(ObjectOperand::Bound { binding: candidate }),
    };
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            Expr::Compute(Box::new(Predicate::NamesIntersect {
                left: Expr::Live(Box::new(NamesRead::Names {
                    object: object.clone(),
                })),
                right: Expr::Literal(vec![name.clone()]),
            })),
            Expr::Live(Box::new(BoolRead::State {
                object,
                property: BooleanState::AdvanceFollower,
            })),
        ],
    }))
}

fn choice(keys: &Keys, name: &CardName) -> Decision {
    let candidate = Binding::new(1, keys.candidate);
    Decision {
        id: keys.decision,
        actor: PlayerRef::Controller {},
        captures: vec![],
        visibility: Visibility::Only {
            players: controller(),
        },
        kind: DecisionKind::ResolutionChoice {
            may: Declinable::MayDecline,
            cardinality: bounds(),
            precondition: None,
        },
        domain: ChoiceDomain::Objects {
            binding: Binding::new(1, keys.scope),
            target_roles: None,
            constraint: SelectionConstraint {
                cardinality: bounds(),
                distinct_by: None,
                disjoint: Disjointness::Disjoint,
                aggregate_bounds: vec![],
                groups: vec![SelectionGroup {
                    cardinality: bounds(),
                    selector: Selector::Zone {
                        players: controller(),
                        zone: ZoneKind::Evolve,
                        view: SelectionView::Current {},
                        binding: candidate,
                        predicate: Box::new(predicate(candidate, name)),
                    },
                }],
            },
        },
    }
}

pub(crate) fn build(
    keys: &Keys,
    source: &SourceSpan,
    movement_source: &SourceSpan,
    origin: &AbilitySource,
    name: &CardName,
) -> Control {
    Control::Sequence {
        body: Program {
            scope: keys.scope,
            steps: vec![
                Located {
                    source: source.clone(),
                    node: Node::Decision {
                        decision: choice(keys, name),
                    },
                },
                Located {
                    source: movement_source.clone(),
                    node: Node::Instruction {
                        instruction: Box::new(Instruction {
                            id: keys.instruction,
                            receipt: keys.receipt,
                            actor: PlayerRef::Controller {},
                            source: origin.clone(),
                            precondition: None,
                            context: InstructionContext::Effect,
                            action: Action::Move {
                                objects: Selector::Bound {
                                    reference: Binding::new(1, keys.scope),
                                },
                                reason: MoveReason::Put,
                                movement: MoveSpec {
                                    to: MoveDestination::Shared {
                                        zone: ZoneRef {
                                            player: PlayerRef::Controller {},
                                            kind: ZoneKind::Field,
                                        },
                                    },
                                    initial_posture: None,
                                    entry_effects: vec![],
                                    position: None,
                                    order: None,
                                },
                            },
                        }),
                    },
                },
            ],
        },
    }
}

pub(crate) fn name(decision: &Decision) -> Option<CardName> {
    let ChoiceDomain::Objects {
        binding,
        constraint,
        ..
    } = &decision.domain
    else {
        return None;
    };
    let [group] = constraint.groups.as_slice() else {
        return None;
    };
    let Selector::Zone {
        binding: candidate,
        predicate,
        ..
    } = &group.selector
    else {
        return None;
    };
    let Expr::Compute(operation) = predicate.as_ref() else {
        return None;
    };
    let Predicate::And { predicates } = operation.as_ref() else {
        return None;
    };
    let [Expr::Compute(names), _] = predicates.as_slice() else {
        return None;
    };
    let Predicate::NamesIntersect {
        right: Expr::Literal(names),
        ..
    } = names.as_ref()
    else {
        return None;
    };
    let [name] = names.as_slice() else {
        return None;
    };
    if name.0.is_empty() {
        return None;
    }
    let keys = Keys {
        scope: binding.key().0,
        candidate: candidate.key().0,
        decision: decision.id,
        instruction: InstructionId(0),
        receipt: ReceiptId(0),
    };
    (*decision == choice(&keys, name)).then(|| name.clone())
}

#[derive(Clone, Copy)]
pub(crate) struct Projection<'shape> {
    pub body: &'shape Program,
    pub decision: &'shape Decision,
    pub candidate: Binding<ObjectRef>,
    pub selected: Binding<Selector>,
    pub instruction: &'shape Instruction,
}

pub(crate) fn projection(control: &Control) -> Option<Projection<'_>> {
    let Control::Sequence { body } = control else {
        return None;
    };
    let [first, second] = body.steps.as_slice() else {
        return None;
    };
    let Node::Decision { decision } = &first.node else {
        return None;
    };
    let Node::Instruction { instruction } = &second.node else {
        return None;
    };
    let name = name(decision)?;
    let ChoiceDomain::Objects {
        binding,
        constraint,
        ..
    } = &decision.domain
    else {
        return None;
    };
    let Selector::Zone {
        binding: candidate, ..
    } = &constraint.groups.first()?.selector
    else {
        return None;
    };
    let keys = Keys {
        scope: body.scope,
        candidate: candidate.key().0,
        decision: decision.id,
        instruction: instruction.id,
        receipt: instruction.receipt,
    };
    (*control
        == build(
            &keys,
            &first.source,
            &second.source,
            &instruction.source,
            &name,
        ))
    .then_some(Projection {
        body,
        decision,
        candidate: *candidate,
        selected: *binding,
        instruction,
    })
}
