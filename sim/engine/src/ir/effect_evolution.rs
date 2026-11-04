//! Closed resolution-time selection and ordinary effect-evolution projection.
use super::{
    AbilitySource, Action, Binding, Cardinality, ChoiceDomain, Control, Decision, DecisionId,
    DecisionKind, Declinable, Disjointness, EvolutionCardRef, EvolutionMode, FaceId, Instruction,
    InstructionContext, InstructionId, Located, Nat, Node, ObjectOperand, ObjectRef, PlayerRef,
    PlayerRelation, PlayerSet, Program, ReceiptId, ScopeId, SelectionConstraint, SelectionGroup,
    SelectionView, Selector, SourceSpan, TextQuantifier, Visibility, ZoneKind,
};

use super::evolution_destination::Destination;
use super::{Expr, FollowPermission, MovementReceipt, NamesRead, ObjectView, Predicate};

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

#[derive(Clone, Debug)]
pub(crate) struct Specification {
    pub subject: ObjectOperand,
    pub destination: Destination,
}

impl Specification {
    pub(crate) fn cardinality(&self) -> Cardinality {
        Cardinality {
            minimum: Nat(u64::from(self.subject == (ObjectOperand::SourceHolder {}))),
            maximum: Some(Nat(1)),
        }
    }
}

fn choice(keys: &Keys, specification: &Specification) -> Decision {
    let candidate = Binding::new(1, keys.candidate);
    let bounds = specification.cardinality();
    Decision {
        id: keys.decision,
        captures: vec![],
        actor: PlayerRef::Controller {},
        visibility: Visibility::Only {
            players: controller(),
        },
        kind: DecisionKind::ResolutionChoice {
            may: if bounds.minimum == Nat(1) {
                Declinable::Required
            } else {
                Declinable::MayDecline
            },
            cardinality: bounds,
            precondition: None,
        },
        domain: ChoiceDomain::Objects {
            binding: Binding::new(1, keys.scope),
            target_roles: None,
            constraint: SelectionConstraint {
                groups: vec![SelectionGroup {
                    cardinality: bounds,
                    selector: Selector::Zone {
                        players: controller(),
                        zone: ZoneKind::Evolve,
                        view: SelectionView::Current {},
                        binding: candidate,
                        predicate: Box::new(
                            specification
                                .destination
                                .predicate_for(candidate, &specification.subject),
                        ),
                    },
                }],
                cardinality: bounds,
                distinct_by: None,
                disjoint: Disjointness::Disjoint,
                aggregate_bounds: vec![],
            },
        },
    }
}

pub(crate) fn build(
    keys: &Keys,
    source: &SourceSpan,
    origin: &AbilitySource,
    specification: &Specification,
) -> Control {
    Control::Sequence {
        body: Program {
            scope: keys.scope,
            steps: vec![
                Located {
                    source: source.clone(),
                    node: Node::Decision {
                        decision: choice(keys, specification),
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
                                                subject: specification.subject.clone(),
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
    specification(decision).is_some()
}

pub(crate) fn specification(decision: &Decision) -> Option<Specification> {
    let ChoiceDomain::Objects {
        constraint,
        binding,
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
    let Predicate::NamesIntersect { right, .. } = operation.as_ref() else {
        return None;
    };
    let subject = if let Expr::Live(read) = right {
        let NamesRead::Names {
            object: ObjectView::Current { object },
        } = read.as_ref()
        else {
            return None;
        };
        object.as_ref().clone()
    } else if matches!(right, Expr::Literal(_)) {
        ObjectOperand::SourceHolder {}
    } else {
        return None;
    };
    if !matches!(
        subject,
        ObjectOperand::SourceHolder {}
            | ObjectOperand::Selected { .. }
            | ObjectOperand::Bound { .. }
    ) {
        return None;
    }
    let destination = Destination::from_predicate_for(predicate, *candidate, &subject)?;
    if !matches!(&destination, Destination::SameName)
        && !matches!(&destination, Destination::Names(names) if names.len() == 1)
    {
        return None;
    }
    let keys = Keys {
        scope: binding.key().0,
        candidate: candidate.key().0,
        body: ScopeId(0),
        decision: decision.id,
        instruction: InstructionId(0),
        receipt: ReceiptId(0),
    };
    let specification = Specification {
        subject,
        destination,
    };
    (*decision == choice(&keys, &specification)).then_some(specification)
}

pub(crate) struct Entry<'entry> {
    pub input: &'entry Selector,
    pub binding: Binding<ObjectRef>,
    pub body: &'entry Program,
}

pub(crate) struct Projection<'projection> {
    pub entry: Option<Entry<'projection>>,
    pub body: &'projection Program,
    pub decision: &'projection Decision,
    pub form: Binding<ObjectRef>,
    pub instruction: &'projection Instruction,
}

fn inner_projection(control: &Control) -> Option<Projection<'_>> {
    let Control::Sequence { body } = control else {
        return None;
    };
    let [first, iteration] = body.steps.as_slice() else {
        return None;
    };
    let Node::Decision { decision } = &first.node else {
        return None;
    };
    let specification = specification(decision)?;
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
    (*control == build(&keys, &first.source, &instruction.source, &specification)).then_some(
        Projection {
            entry: None,
            body,
            decision,
            form: *binding,
            instruction,
        },
    )
}

pub(crate) fn field_entry(
    input: Selector,
    binding: Binding<ObjectRef>,
    control: Control,
    source: &SourceSpan,
) -> Control {
    Control::EachObject {
        input,
        binding,
        order: None,
        body: Program {
            scope: binding.key().0,
            steps: vec![Located {
                source: source.clone(),
                node: Node::Control { control },
            }],
        },
    }
}

pub(crate) fn projection(control: &Control) -> Option<Projection<'_>> {
    if let Some(projection) = inner_projection(control) {
        return Some(projection);
    }
    let Control::EachObject {
        input,
        binding,
        body,
        order: None,
    } = control
    else {
        return None;
    };
    if !matches!(
        input,
        Selector::MoveAfter {
            receipt: MovementReceipt::Search { .. },
            permission: FollowPermission::SameEffectExplicitReference {},
        }
    ) {
        return None;
    }
    let [step] = body.steps.as_slice() else {
        return None;
    };
    let Node::Control { control: inner } = &step.node else {
        return None;
    };
    let mut projection = inner_projection(inner)?;
    if !matches!(&projection.instruction.action, Action::Evolve { subject: ObjectOperand::Bound { binding: subject }, .. } if subject == binding)
        || *control
            != field_entry(
                input.clone(),
                *binding,
                inner.clone(),
                &projection.body.steps.first()?.source,
            )
    {
        return None;
    }
    projection.entry = Some(Entry {
        input,
        binding: *binding,
        body,
    });
    Some(projection)
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
                Control::Sequence { body } | Control::May { body, .. } => mask(body, origin),
                Control::Pay { on_paid, .. } => mask(on_paid, origin),
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
                | Control::Repeat { .. }
                | Control::EachPlayer { .. }
                | Control::EachObject { .. } => {}
            }
        }
    }
}
