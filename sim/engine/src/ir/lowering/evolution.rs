//! Intrinsic evolution is a fixed play projection, not a general Evolve body permission.
use super::{Allocator, LoweringError, ast, source_span, unsupported};
use crate::ir::effect_evolution;
use crate::ir::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Action,
    Binding, CardName, Cardinality, ChoiceDomain, Control, CostSpec, CostStep, Decision,
    DecisionId, DecisionKind, Disjointness, EvolutionCardRef, EvolutionMode, ExecutionRef, Expr,
    FaceId, InputMode, Instruction, InstructionContext, InstructionId, Int, KeywordId, Located,
    NamesRead, Nat, Node, ObjectOperand, ObjectView, PlaySlot, PlaySpec, PlayerRef, PlayerRelation,
    PlayerSet, Predicate, Program, ReceiptId, Resource, ResourceOperation, ScopeId,
    SelectionConstraint, SelectionGroup, SelectionSnapshot, SelectionView, Selector,
    TextQuantifier, Visibility, ZoneKind,
};
use crate::ir::{Precondition, PreconditionKind, PreconditionReason};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Effect evolution uses the same authority-scoped evolution projection."
)]
impl super::BodyContext<'_> {
    pub(super) fn effect_evolution(
        &mut self,
        path: &str,
        source: &super::SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        let mut source = source.clone();
        source.author_path = path.into();
        let id = self.card_allocator.id(path)?;
        let keys = effect_evolution::Keys {
            scope: self.card_allocator.scope(path)?,
            candidate: self.card_allocator.scope(path)?,
            body: self.card_allocator.scope(path)?,
            decision: DecisionId(self.card_allocator.id(path)?),
            instruction: InstructionId(id),
            receipt: ReceiptId(id),
        };
        Ok(Located {
            node: Node::Control {
                control: effect_evolution::build(&keys, &source, origin),
            },
            source,
        })
    }
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
fn names(object: ObjectOperand) -> Expr<Vec<CardName>> {
    Expr::Live(Box::new(NamesRead::Names {
        object: ObjectView::Current {
            object: Box::new(object),
        },
    }))
}

fn face(scope: ScopeId, candidate_scope: ScopeId, id: u64) -> Decision {
    let candidate = Binding::new(3, candidate_scope);
    Decision {
        id: DecisionId(id),
        captures: vec![],
        actor: PlayerRef::Controller {},
        visibility: Visibility::Only {
            players: controller(),
        },
        kind: DecisionKind::PlayParameter {
            slot: PlaySlot::Face,
            selection_snapshot: SelectionSnapshot::AtDecision,
            play_scope: ExecutionRef::Current {},
            input: InputMode::Inline,
        },
        domain: ChoiceDomain::Objects {
            binding: Binding::new(1, scope),
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
                disjoint: Disjointness::Disjoint,
                distinct_by: None,
                aggregate_bounds: vec![],
            },
        },
    }
}

fn instruction(
    action: Action,
    id: u64,
    source: &AbilitySource,
    context: InstructionContext,
) -> Instruction {
    Instruction {
        id: InstructionId(id),
        receipt: ReceiptId(id),
        actor: PlayerRef::Controller {},
        source: source.clone(),
        precondition: None,
        context,
        action,
    }
}

fn costs(
    allocator: &mut Allocator,
    selection: Binding<Selector>,
    pp: i64,
    source: &AbilitySource,
    path: &str,
) -> Result<CostSpec, LoweringError> {
    let reveal = instruction(
        Action::Reveal {
            objects: Selector::Bound {
                reference: selection,
            },
        },
        allocator.id(path)?,
        source,
        InstructionContext::Cost,
    );
    let pay = instruction(
        Action::Resource {
            player: PlayerRef::Controller {},
            resource: Resource::PpCurrent,
            operation: ResourceOperation::Pay,
            amount: Expr::Literal(Int(pp)),
        },
        allocator.id(path)?,
        source,
        InstructionContext::Cost,
    );
    Ok(CostSpec::Ordered {
        steps: vec![
            CostStep {
                instruction: Box::new(reveal),
                materials: None,
            },
            CostStep {
                instruction: Box::new(pay),
                materials: None,
            },
        ],
    })
}

fn qualification(
    head: &ast::Evolution,
    scope: ScopeId,
    protects: InstructionId,
    path: &str,
) -> Result<Option<Precondition>, LoweringError> {
    let Some(condition) = &head.usable_if else {
        return Ok(None);
    };
    let ast::Condition::Exists(query) = condition else {
        return Err(unsupported(path));
    };
    if !matches!(query.who, ast::Side::Controller)
        || !matches!(query.zone, ast::PublicZone::Field)
        || query.other
        || query.class.is_some()
        || query.token.is_some()
    {
        return Err(unsupported(path));
    }
    Ok(Some(Precondition {
        kind: PreconditionKind::CardTextPrerequisite,
        protects,
        reason: PreconditionReason::CardText {},
        predicate: Expr::Compute(Box::new(Predicate::Exists {
            objects: super::costs::public_query(query, Binding::new(4, scope)),
        })),
    }))
}

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    if entry.fanfare.is_some()
        || entry.lastword.is_some()
        || entry.attack_time.is_some()
        || entry.auto.is_some()
        || entry.evolve_time.is_some()
        || entry.activate.is_some()
        || entry.spell.is_some()
    {
        return Err(unsupported(path));
    }
    let head = entry.evolve.as_ref().ok_or_else(|| unsupported(path))?;
    let pp = i64::try_from(head.cost.pp).map_err(|_private| unsupported(path))?;
    let source = source_span(key, path, entry)?;
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        controller: PlayerRef::Controller {},
        origin: AbilityOrigin::Keyword {
            keyword: KeywordId::Evolution,
            origin: Box::new(AbilityOrigin::Printed {
                face: FaceId(source.face.clone()),
                line: Nat(u64::from(entry.line)),
            }),
        },
    };
    let candidate_scope = ScopeId(scope.0.checked_add(1).ok_or_else(|| unsupported(path))?);
    let parameter = face(scope, candidate_scope, allocator.id(path)?);
    let selection = Binding::new(1, scope);
    let inner_scope = ScopeId(scope.0.checked_add(2).ok_or_else(|| unsupported(path))?);
    let element = Binding::new(1, inner_scope);
    let mut costs = costs(allocator, selection, pp, &origin, path)?;
    if let CostSpec::Ordered { steps } = &mut costs {
        let reveal = steps.first_mut().ok_or_else(|| unsupported(path))?;
        reveal.instruction.precondition = qualification(head, scope, reveal.instruction.id, path)?;
    }
    let evolve = instruction(
        Action::Evolve {
            subject: ObjectOperand::SourceHolder {},
            form: EvolutionCardRef {
                object: ObjectOperand::Bound { binding: element },
                face: FaceId("base".into()),
            },
            mode: EvolutionMode::Normal,
            link: Binding::new(2, inner_scope),
        },
        allocator.id(path)?,
        &origin,
        InstructionContext::Effect,
    );
    Ok(Located {
        source: source.clone(),
        node: Ability {
            id: AbilityId(scope.0),
            scope,
            source: origin,
            kind: AbilityKind::Activated,
            qualification: AbilityQualification::Ordinary,
            active_zones: vec![ZoneKind::Field],
            event_captures: vec![],
            declarations: vec![],
            trigger: None,
            play: Some(PlaySpec {
                limit: None,
                captures: vec![],
                parameters: vec![parameter],
                costs,
            }),
            body: Some(Program {
                scope,
                steps: vec![Located {
                    source: source.clone(),
                    node: Node::Control {
                        control: Control::EachObject {
                            input: Selector::Bound {
                                reference: selection,
                            },
                            binding: element,
                            order: None,
                            body: Program {
                                scope: inner_scope,
                                steps: vec![Located {
                                    source,
                                    node: Node::Instruction {
                                        instruction: Box::new(evolve),
                                    },
                                }],
                            },
                        },
                    },
                }],
            }),
        },
    })
}
