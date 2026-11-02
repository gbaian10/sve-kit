use super::{has_keyword, players, view};
use crate::ir::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Action,
    Binding, CardKind, Cardinality, CharacteristicView, ChoiceDomain, CostSpec, CostStep,
    CounterKind, CounterOperation, Decision, DecisionId, DecisionKind, Disjointness, ExecutionRef,
    Expr, FaceId, Holder, InputMode, Instruction, InstructionContext, InstructionId, KeywordId,
    KindRead, Located, Nat, NatRead, Node, ObjectOperand, ObjectRef, PlaySlot, PlaySpec, PlayerRef,
    PlayerRelation, Posture, Predicate, Program, ReceiptId, RuleId, RuleProcedure, ScopeId,
    SelectionConstraint, SelectionGroup, SelectionSnapshot, SelectionView, Selector, SourceSpan,
    Visibility, ZoneKind,
};

pub(crate) fn recipients(scope: ScopeId, other: bool) -> Selector {
    let object = Binding::<ObjectRef>::new(1, scope);
    let cards = Selector::Zone {
        players: players(PlayerRelation::SelfPlayer),
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding: object,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::KindEquals {
                    left: Expr::Live(Box::new(KindRead::Kind {
                        object: view(ObjectOperand::Bound { binding: object }),
                        view: CharacteristicView::Effective,
                    })),
                    right: Expr::Literal(CardKind::Amulet),
                })),
                has_keyword(ObjectOperand::Bound { binding: object }, KeywordId::Stack),
            ],
        }))),
    };
    if other {
        Selector::Difference {
            input: Box::new(cards),
            except: Box::new(Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            }),
        }
    } else {
        cards
    }
}

pub(crate) fn recipient_choice(id: DecisionId, scope: ScopeId) -> Decision {
    let cardinality = Cardinality {
        minimum: Nat(1),
        maximum: Some(Nat(1)),
    };
    Decision {
        id,
        captures: vec![],
        actor: PlayerRef::Controller {},
        domain: ChoiceDomain::Objects {
            binding: Binding::new(0, scope),
            constraint: SelectionConstraint {
                groups: vec![SelectionGroup {
                    selector: recipients(scope, true),
                    cardinality,
                }],
                cardinality,
                distinct_by: None,
                disjoint: Disjointness::Disjoint,
                aggregate_bounds: vec![],
            },
            target_roles: None,
        },
        kind: DecisionKind::PlayParameter {
            slot: PlaySlot::Targets,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        visibility: Visibility::Public {},
    }
}

/// CR 13.3.2.2 grants the same activation to every Stack amulet.
pub(crate) fn stack_definition(
    id: AbilityId,
    scope: ScopeId,
    decision: DecisionId,
    cost: InstructionId,
    effect: InstructionId,
    span: &SourceSpan,
) -> Ability {
    let source = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        controller: PlayerRef::Controller {},
        origin: AbilityOrigin::Keyword {
            keyword: KeywordId::Stack,
            origin: Box::new(AbilityOrigin::Printed {
                face: FaceId(span.face.clone()),
                line: Nat(u64::from(span.line)),
            }),
        },
    };
    let holder = Holder::Object {
        object: ObjectOperand::SourceHolder {},
    };
    let kind = CounterKind("stack_counter".into());
    Ability {
        id,
        scope,
        source: source.clone(),
        kind: AbilityKind::Activated,
        qualification: AbilityQualification::Ordinary,
        active_zones: vec![ZoneKind::Field],
        event_captures: vec![],
        play: Some(PlaySpec {
            limit: None,
            captures: vec![],
            parameters: vec![recipient_choice(decision, scope)],
            costs: CostSpec::Ordered {
                steps: vec![CostStep {
                    keyword: None,
                    materials: None,
                    instruction: Box::new(Instruction {
                        id: cost,
                        actor: PlayerRef::Controller {},
                        source: source.clone(),
                        precondition: None,
                        context: InstructionContext::Cost,
                        receipt: ReceiptId(cost.0),
                        action: Action::Posture {
                            objects: Selector::Objects {
                                objects: vec![ObjectOperand::SourceHolder {}],
                            },
                            posture: Posture::Acted,
                        },
                    }),
                }],
            },
        }),
        trigger: None,
        declarations: vec![],
        body: Some(Program {
            scope,
            steps: vec![Located {
                source: span.clone(),
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: effect,
                        actor: PlayerRef::Controller {},
                        source,
                        precondition: None,
                        context: InstructionContext::Effect,
                        receipt: ReceiptId(effect.0),
                        action: Action::Counter {
                            holder: holder.clone(),
                            kind: kind.clone(),
                            operation: CounterOperation::Move {
                                to: Holder::Object {
                                    object: ObjectOperand::Selected {
                                        selection: Binding::new(0, scope),
                                    },
                                },
                            },
                            amount: Expr::Live(Box::new(NatRead::Counter { holder, kind })),
                        },
                    }),
                },
            }],
        }),
    }
}

pub(super) fn rebuild(ability: &Ability) -> Option<Ability> {
    let play = ability.play.as_ref()?;
    let CostSpec::Ordered { steps: costs } = &play.costs else {
        return None;
    };
    let step = ability.body.as_ref()?.steps.first()?;
    let Node::Instruction { instruction } = &step.node else {
        return None;
    };
    Some(stack_definition(
        ability.id,
        ability.scope,
        play.parameters.first()?.id,
        costs.first()?.instruction.id,
        instruction.id,
        &step.source,
    ))
}

/// Stack increase chooses a recipient under the rule, including a singleton domain.
pub(crate) fn increase_choice(id: DecisionId, scope: ScopeId) -> Decision {
    let mut choice = recipient_choice(id, scope);
    if let ChoiceDomain::Objects { constraint, .. } = &mut choice.domain
        && let Some(group) = constraint.groups.first_mut()
    {
        group.selector = recipients(scope, false);
    }
    choice.kind = DecisionKind::RuleChoice {
        procedure: RuleProcedure::StackRecipient,
        rule: RuleId(13324),
    };
    choice
}

pub(crate) fn is_increase_choice(choice: &Decision) -> bool {
    let ChoiceDomain::Objects { binding, .. } = choice.domain else {
        return false;
    };
    *choice == increase_choice(choice.id, binding.key().0)
}
