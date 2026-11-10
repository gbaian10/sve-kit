//! A private named Advance choice authorizes only its adjacent move or normal spell play.
use super::{
    AbilitySource, Action, Binding, Bool, BoolRead, BooleanState, CardName, Cardinality,
    ChoiceDomain, Control, CostSpec, Decision, DecisionId, DecisionKind, Declinable, Disjointness,
    EffectReference, Expr, Instruction, InstructionContext, InstructionId, Located,
    MoveDestination, MoveReason, MoveSpec, NamesRead, Nat, Node, ObjectOperand, ObjectRef,
    ObjectView, OwnerZone, PlaySpec, PlayerRef, PlayerRelation, PlayerSet, Predicate, Program,
    ReceiptId, ScopeId, SelectionConstraint, SelectionGroup, SelectionView, Selector, SourceSpan,
    TextQuantifier, Visibility, ZoneKind, ZoneRef,
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

/// The printed name a named Advance pick must match: the whole name, or a fragment of it (BP10-063).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AdvanceName {
    Exact(CardName),
    Contains(CardName),
}

impl AdvanceName {
    /// Whether an own printed rules name satisfies this pick.
    pub(crate) fn matches(&self, own: &str) -> bool {
        match self {
            Self::Exact(name) => own == name.0,
            Self::Contains(fragment) => own.contains(&fragment.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdvanceKind {
    Follower,
    Spell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdvanceSelection {
    pub name: AdvanceName,
    pub kind: AdvanceKind,
}

#[derive(Clone, Copy)]
pub(crate) enum AdvanceUse {
    Field,
    Ex,
    PlaySpell,
}

impl AdvanceUse {
    const fn kind(self) -> AdvanceKind {
        match self {
            Self::Field | Self::Ex => AdvanceKind::Follower,
            Self::PlaySpell => AdvanceKind::Spell,
        }
    }
}

fn predicate(candidate: Binding<ObjectRef>, selection: &AdvanceSelection) -> Expr<Bool> {
    let object = ObjectView::Current {
        object: Box::new(ObjectOperand::Bound { binding: candidate }),
    };
    let names = match &selection.name {
        AdvanceName::Exact(name) => Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: object.clone(),
            })),
            right: Expr::Literal(vec![name.clone()]),
        },
        AdvanceName::Contains(fragment) => Predicate::NameContains {
            object: object.clone(),
            name: Expr::Literal(fragment.clone()),
        },
    };
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            Expr::Compute(Box::new(names)),
            Expr::Live(Box::new(BoolRead::State {
                object,
                property: match selection.kind {
                    AdvanceKind::Follower => BooleanState::AdvanceFollower,
                    AdvanceKind::Spell => BooleanState::AdvanceSpell,
                },
            })),
        ],
    }))
}

fn choice(keys: &Keys, selection: &AdvanceSelection) -> Decision {
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
                        predicate: Box::new(predicate(candidate, selection)),
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
    name: &AdvanceName,
    usage: AdvanceUse,
) -> Control {
    let selection = AdvanceSelection {
        name: name.clone(),
        kind: usage.kind(),
    };
    let action = match usage {
        AdvanceUse::PlaySpell => Action::PlayCard {
            card: ObjectOperand::Selected {
                selection: Binding::new(1, keys.scope),
            },
            controller: PlayerRef::Controller {},
            cost_override: None,
            authorization: EffectReference::CurrentInstallation {},
            parameters: PlaySpec {
                limit: None,
                captures: vec![],
                parameters: vec![],
                costs: CostSpec::Ordered { steps: vec![] },
            },
        },
        AdvanceUse::Field | AdvanceUse::Ex => move_action(keys, usage),
    };
    Control::Sequence {
        body: Program {
            scope: keys.scope,
            steps: vec![
                Located {
                    source: source.clone(),
                    node: Node::Decision {
                        decision: choice(keys, &selection),
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
                            action,
                        }),
                    },
                },
            ],
        },
    }
}

const fn move_action(keys: &Keys, usage: AdvanceUse) -> Action {
    Action::Move {
        objects: Selector::Bound {
            reference: Binding::new(1, keys.scope),
        },
        reason: MoveReason::Put,
        movement: MoveSpec {
            // EX uses the owner's zone, like other own cards put there.
            to: if matches!(usage, AdvanceUse::Ex) {
                MoveDestination::Owners {
                    zone: OwnerZone::Ex,
                }
            } else {
                MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Field,
                    },
                }
            },
            initial_posture: None,
            entry_effects: vec![],
            position: None,
            order: None,
        },
    }
}

pub(crate) fn selection(decision: &Decision) -> Option<AdvanceSelection> {
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
    let [Expr::Compute(names), Expr::Live(state)] = predicates.as_slice() else {
        return None;
    };
    let name = if let Predicate::NamesIntersect {
        right: Expr::Literal(names),
        ..
    } = names.as_ref()
    {
        let [name] = names.as_slice() else {
            return None;
        };
        AdvanceName::Exact(name.clone())
    } else if let Predicate::NameContains {
        name: Expr::Literal(fragment),
        ..
    } = names.as_ref()
    {
        AdvanceName::Contains(fragment.clone())
    } else {
        return None;
    };
    let (AdvanceName::Exact(text) | AdvanceName::Contains(text)) = &name;
    if text.0.is_empty() {
        return None;
    }
    let keys = Keys {
        scope: binding.key().0,
        candidate: candidate.key().0,
        decision: decision.id,
        instruction: InstructionId(0),
        receipt: ReceiptId(0),
    };
    let BoolRead::State { property, .. } = state.as_ref() else {
        return None;
    };
    let kind = match property {
        BooleanState::AdvanceFollower => AdvanceKind::Follower,
        BooleanState::AdvanceSpell if matches!(name, AdvanceName::Exact(_)) => AdvanceKind::Spell,
        BooleanState::TokenStatus
        | BooleanState::FaceUp
        | BooleanState::Boxed
        | BooleanState::Racing
        | BooleanState::AdvanceSpell
        | BooleanState::EnteredThisTurn => return None,
    };
    let selection = AdvanceSelection { name, kind };
    (*decision == choice(&keys, &selection)).then_some(selection)
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
    let selection = selection(decision)?;
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
    // The complete pair seals the private choice to its only authorized consumer.
    [AdvanceUse::Field, AdvanceUse::Ex, AdvanceUse::PlaySpell]
        .into_iter()
        .any(|usage| {
            *control
                == build(
                    &keys,
                    &first.source,
                    &second.source,
                    &instruction.source,
                    &selection.name,
                    usage,
                )
        })
        .then_some(Projection {
            body,
            decision,
            candidate: *candidate,
            selected: *binding,
            instruction,
        })
}
