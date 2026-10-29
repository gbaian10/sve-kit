//! Closed resolution-time selection and ordinary self-evolution projection.
use super::{
    AbilitySource, Action, Binding, Cardinality, ChoiceDomain, Control, Decision, DecisionId,
    DecisionKind, Declinable, Disjointness, EvolutionCardRef, EvolutionMode, Expr, FaceId,
    Instruction, InstructionContext, InstructionId, Located, NamesRead, Nat, Node, ObjectOperand,
    ObjectRef, ObjectView, PlayerRef, PlayerRelation, PlayerSet, Predicate, Program, ReceiptId,
    ScopeId, SelectionConstraint, SelectionGroup, SelectionView, Selector, SourceSpan,
    TextQuantifier, Visibility, ZoneKind,
};

pub(crate) struct Keys {
    pub scope: ScopeId,
    pub candidate: ScopeId,
    pub body: ScopeId,
    pub decision: DecisionId,
    pub instruction: InstructionId,
    pub receipt: ReceiptId,
}

const fn one() -> Cardinality {
    Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    }
}

const fn controller() -> PlayerSet {
    PlayerSet {
        relation: PlayerRelation::SelfPlayer,
        relative_to: PlayerRef::Controller {},
        cardinality: one(),
        text_quantifier: TextQuantifier::Implicit,
    }
}

fn names(object: ObjectOperand) -> Expr<Vec<super::CardName>> {
    Expr::Live(Box::new(NamesRead::Names {
        object: ObjectView::Current {
            object: Box::new(object),
        },
    }))
}

fn choice(keys: &Keys) -> Decision {
    let candidate = Binding::new(1, keys.candidate);
    Decision {
        id: keys.decision,
        captures: vec![],
        actor: PlayerRef::Controller {},
        visibility: Visibility::Only {
            players: controller(),
        },
        kind: DecisionKind::ResolutionChoice {
            may: Declinable::Required,
            cardinality: one(),
            precondition: None,
        },
        domain: ChoiceDomain::Objects {
            binding: Binding::new(1, keys.scope),
            target_roles: None,
            constraint: SelectionConstraint {
                groups: vec![SelectionGroup {
                    cardinality: one(),
                    selector: Selector::Zone {
                        players: controller(),
                        zone: ZoneKind::Evolve,
                        view: SelectionView::Current {},
                        binding: candidate,
                        predicate: Box::new(Expr::Compute(Box::new(Predicate::NamesIntersect {
                            left: names(ObjectOperand::Bound { binding: candidate }),
                            right: names(ObjectOperand::SourceHolder {}),
                        }))),
                    },
                }],
                cardinality: one(),
                distinct_by: None,
                disjoint: Disjointness::Disjoint,
                aggregate_bounds: vec![],
            },
        },
    }
}

pub(crate) fn build(keys: &Keys, source: &SourceSpan, origin: &AbilitySource) -> Control {
    Control::Sequence {
        body: Program {
            scope: keys.scope,
            steps: vec![
                Located {
                    source: source.clone(),
                    node: Node::Decision {
                        decision: choice(keys),
                    },
                },
                Located {
                    source: source.clone(),
                    node: Node::Control {
                        control: Control::EachObject {
                            input: Selector::Bound {
                                reference: Binding::new(1, keys.scope),
                            },
                            binding: Binding::new(1, keys.body),
                            order: None,
                            body: Program {
                                scope: keys.body,
                                steps: vec![Located {
                                    source: source.clone(),
                                    node: Node::Instruction {
                                        instruction: Box::new(Instruction {
                                            id: keys.instruction,
                                            receipt: keys.receipt,
                                            actor: PlayerRef::Controller {},
                                            source: origin.clone(),
                                            context: InstructionContext::Effect,
                                            precondition: None,
                                            action: Action::Evolve {
                                                subject: ObjectOperand::SourceHolder {},
                                                form: EvolutionCardRef {
                                                    object: ObjectOperand::Bound {
                                                        binding: Binding::new(1, keys.body),
                                                    },
                                                    face: FaceId("base".into()),
                                                },
                                                mode: EvolutionMode::Normal,
                                                link: Binding::new(2, keys.body),
                                            },
                                        }),
                                    },
                                }],
                            },
                        },
                    },
                },
            ],
        },
    }
}

pub(crate) fn is_choice(decision: &Decision) -> bool {
    let ChoiceDomain::Objects {
        constraint,
        binding,
        ..
    } = &decision.domain
    else {
        return false;
    };
    let [group] = constraint.groups.as_slice() else {
        return false;
    };
    let Selector::Zone {
        binding: candidate, ..
    } = &group.selector
    else {
        return false;
    };
    let keys = Keys {
        scope: binding.key().0,
        candidate: candidate.key().0,
        body: ScopeId(0),
        decision: decision.id,
        instruction: InstructionId(0),
        receipt: ReceiptId(0),
    };
    *decision == choice(&keys)
}

pub(crate) struct Projection<'projection> {
    pub body: &'projection Program,
    pub decision: &'projection Decision,
    pub form: Binding<ObjectRef>,
    pub instruction: &'projection Instruction,
}

pub(crate) fn projection(control: &Control) -> Option<Projection<'_>> {
    let Control::Sequence { body } = control else {
        return None;
    };
    let [first, iteration] = body.steps.as_slice() else {
        return None;
    };
    let Node::Decision { decision } = &first.node else {
        return None;
    };
    if !is_choice(decision) {
        return None;
    }
    let Node::Control {
        control:
            Control::EachObject {
                body: inner,
                binding,
                ..
            },
    } = &iteration.node
    else {
        return None;
    };
    let [step] = inner.steps.as_slice() else {
        return None;
    };
    let Node::Instruction { instruction } = &step.node else {
        return None;
    };
    let ChoiceDomain::Objects { constraint, .. } = &decision.domain else {
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
        body: inner.scope,
        decision: decision.id,
        instruction: instruction.id,
        receipt: instruction.receipt,
    };
    (*control == build(&keys, &first.source, &instruction.source)).then_some(Projection {
        body,
        decision,
        form: *binding,
        instruction,
    })
}

pub(crate) fn mask(program: &mut Program, origin: &AbilitySource) {
    for step in &mut program.steps {
        if let Node::Control { control } = &mut step.node {
            if let Some(projection) = projection(control)
                && projection.instruction.source == *origin
            {
                *control = Control::Sequence {
                    body: Program {
                        scope: projection.body.scope,
                        steps: vec![],
                    },
                };
                continue;
            }
            match control {
                Control::Sequence { body } => mask(body, origin),
                Control::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    mask(then_body, origin);
                    if let Some(body) = else_body {
                        mask(body, origin);
                    }
                }
                Control::Mode { modes, .. } => {
                    for mode in modes {
                        mask(&mut mode.body, origin);
                    }
                }
                Control::Simultaneous { .. }
                | Control::Match { .. }
                | Control::May { .. }
                | Control::Repeat { .. }
                | Control::EachPlayer { .. }
                | Control::EachObject { .. }
                | Control::Pay { .. } => {}
            }
        }
    }
}
