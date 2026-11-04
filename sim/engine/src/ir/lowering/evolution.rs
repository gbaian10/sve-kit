//! Intrinsic evolution is a fixed play projection, not a general Evolve body permission.
use super::{Allocator, LoweringError, ast, source_span, unsupported};
use crate::ir::effect_evolution;
use crate::ir::events::subject::{SubjectFields, SubjectFilter};
use crate::ir::evolution_destination::Destination;
use crate::ir::{
    Ability, AbilityId, AbilityKind, AbilityOrigin, AbilityQualification, AbilitySource, Action,
    Binding, CardName, Cardinality, ChoiceDomain, Control, CostSpec, CostStep, Decision,
    DecisionId, DecisionKind, Disjointness, EvolutionCardRef, EvolutionMode, ExecutionRef, FaceId,
    InputMode, Instruction, InstructionContext, InstructionId, KeywordId, Located, Nat, Node,
    ObjectOperand, PlaySlot, PlaySpec, PlayerRef, PlayerRelation, PlayerSet, Program, ReceiptId,
    ScopeId, SelectionConstraint, SelectionGroup, SelectionSnapshot, SelectionView, Selector,
    TextQuantifier, Visibility, ZoneKind,
};
use crate::ir::{CardKind, EventPattern, FollowPermission, MovementReceipt, TraitId};

#[expect(
    clippy::multiple_inherent_impl,
    reason = "Effect evolution uses the same authority-scoped evolution projection."
)]
impl super::BodyContext<'_> {
    pub(super) fn effect_evolution(
        &mut self,
        evolution: &ast::NamedEffectEvolution,
        path: &str,
        source: &super::SourceSpan,
        origin: &AbilitySource,
    ) -> Result<Located<Node>, LoweringError> {
        if matches!(
            evolution.into,
            Some(ast::EvolutionDestination::Names(_) | ast::EvolutionDestination::Contains(_))
        ) || matches!(&evolution.into, Some(ast::EvolutionDestination::Name(name)) if name.is_empty())
        {
            return Err(unsupported(path));
        }
        let (subject, input) = self.effect_evolution_subject(&evolution.reference, path)?;
        if subject != (ObjectOperand::SourceHolder {}) && evolution.into.is_some() {
            return Err(unsupported(path));
        }
        let specification = effect_evolution::Specification {
            subject,
            destination: destination(evolution.into.as_ref(), path)?,
        };
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
        let mut control = effect_evolution::build(&keys, &source, origin, &specification);
        if let Some(input) = input {
            let ObjectOperand::Bound { binding } = specification.subject else {
                return Err(unsupported(path));
            };
            control = effect_evolution::field_entry(input, binding, control, &source);
        }
        Ok(Located {
            node: Node::Control { control },
            source,
        })
    }

    pub(super) fn remember_evolution_target(
        &mut self,
        target: &ast::Target,
        bounds: Cardinality,
        dynamic: bool,
    ) {
        if !dynamic
            && bounds.maximum == Some(Nat(1))
            && matches!(target.from.who, Some(ast::Side::Controller))
            && matches!(
                target.from.zone,
                Some(ast::TargetZone::Single(ast::TargetSingleZone::Field))
            )
            && matches!(target.from.kind, Some(ast::SearchKind::Follower))
        {
            self.evolution_targets.insert(target.bind.clone());
        }
    }

    fn effect_evolution_subject(
        &mut self,
        reference: &ast::Objects,
        path: &str,
    ) -> Result<(ObjectOperand, Option<Selector>), LoweringError> {
        match reference {
            ast::Objects::This(_) => Ok((ObjectOperand::SourceHolder {}, None)),
            ast::Objects::Bound(name) if self.evolution_targets.contains(name) => {
                let selection = self
                    .targets
                    .get(name)
                    .copied()
                    .ok_or_else(|| unsupported(path))?;
                Ok((ObjectOperand::Selected { selection }, None))
            }
            ast::Objects::Follow { follow } => {
                let receipt = self
                    .field_outputs
                    .get(follow)
                    .filter(|receipt| matches!(receipt, MovementReceipt::Search { .. }))
                    .cloned()
                    .ok_or_else(|| unsupported(path))?;
                let binding = Binding::new(3, self.card_allocator.scope(path)?);
                Ok((
                    ObjectOperand::Bound { binding },
                    Some(Selector::MoveAfter {
                        receipt,
                        permission: FollowPermission::SameEffectExplicitReference {},
                    }),
                ))
            }
            ast::Objects::Bound(_) | ast::Objects::Zone(_) | ast::Objects::EventHolder { .. } => {
                Err(unsupported(path))
            }
        }
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

fn face(scope: ScopeId, candidate_scope: ScopeId, id: u64, destination: &Destination) -> Decision {
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
                        predicate: Box::new(destination.predicate(candidate)),
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

fn intrinsic_origin(
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
) -> Result<(super::SourceSpan, AbilitySource), LoweringError> {
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
    Ok((source, origin))
}

fn destination(
    into: Option<&ast::EvolutionDestination>,
    path: &str,
) -> Result<Destination, LoweringError> {
    Ok(match into {
        None => Destination::SameName,
        Some(ast::EvolutionDestination::Name(name)) => {
            Destination::Names(vec![CardName(name.clone())])
        }
        Some(ast::EvolutionDestination::Names(names)) => {
            Destination::Names(names.iter().cloned().map(CardName).collect())
        }
        Some(ast::EvolutionDestination::Contains(selector)) => {
            let ast::SelfPlayer::Controller = selector.who;
            let ast::EvolutionZone::Evolve = selector.zone;
            if !matches!(selector.kind, ast::SearchKind::EvolveFollower) {
                return Err(unsupported(path));
            }
            Destination::Contains(CardName(selector.name_contains.clone()))
        }
    })
}

pub(super) fn lower(
    allocator: &mut Allocator,
    key: &super::CardCoordinates<'_>,
    path: &str,
    entry: &ast::Ability,
    scope: ScopeId,
) -> Result<Located<Ability>, LoweringError> {
    let (source, origin) = intrinsic_origin(key, path, entry)?;
    let head = entry.evolve.as_ref().ok_or_else(|| unsupported(path))?;
    let candidate_scope = ScopeId(scope.0.checked_add(1).ok_or_else(|| unsupported(path))?);
    let destination = destination(head.into.as_ref(), path)?;
    let parameter = face(scope, candidate_scope, allocator.id(path)?, &destination);
    let selection = Binding::new(1, scope);
    let inner_scope = ScopeId(scope.0.checked_add(2).ok_or_else(|| unsupported(path))?);
    let element = Binding::new(1, inner_scope);
    let body = ast::Body::default();
    let mut context = super::BodyContext::new(entry, scope, &body, None, allocator);
    // Face selection reserves local bindings 1 through 3.
    context.allocator.next = 3;
    let mut payment = super::payment::lower(&mut context, &head.cost, &origin, path)?;
    let predicate = head
        .usable_if
        .as_ref()
        .map(|condition| context.qualification(condition, path))
        .transpose()?;
    let reveal = instruction(
        Action::Reveal {
            objects: Selector::Bound {
                reference: selection,
            },
        },
        context.card_allocator.id(path)?,
        &origin,
        InstructionContext::Cost,
    );
    let CostSpec::Ordered { steps } = &mut payment.costs else {
        return Err(unsupported(path));
    };
    steps.insert(
        0,
        CostStep {
            keyword: None,
            instruction: Box::new(reveal),
            materials: None,
        },
    );
    super::conditions::qualify_costs(&mut payment.costs, predicate, path)?;
    payment.parameters.insert(0, parameter);
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
                parameters: payment.parameters,
                costs: payment.costs,
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

pub(super) fn subscription(
    entry: &ast::EvolvedSubscription,
    event_id: u64,
    scope: ScopeId,
    path: &str,
) -> Result<EventPattern, LoweringError> {
    if entry
        .sel
        .as_ref()
        .is_some_and(|sel| sel.token.is_some() || sel.not_token.is_some() || sel.class.is_some())
    {
        return Err(unsupported(path));
    }
    let event = Binding::new(event_id, scope);
    let filter = SubjectFilter {
        own: entry
            .who
            .as_ref()
            .map(|who| matches!(who, ast::EntryPlayer::Controller)),
        itself: if entry.reference.is_some() {
            Some(true)
        } else {
            entry.sel.as_ref().and_then(|sel| sel.other).map(|_| false)
        },
        kind: entry
            .sel
            .as_ref()
            .and_then(|sel| sel.kind.as_ref())
            .map(|kind| match kind {
                ast::SearchKind::Follower => CardKind::Follower,
                ast::SearchKind::Spell => CardKind::Spell,
                ast::SearchKind::Amulet => CardKind::Amulet,
                ast::SearchKind::EvolveFollower => CardKind::EvolveFollower,
            }),
        trait_id: entry
            .sel
            .as_ref()
            .and_then(|sel| sel.trait_id.as_ref())
            .map(|id| TraitId(id.clone())),
        name: entry
            .sel
            .as_ref()
            .and_then(|sel| sel.name.as_ref())
            .map(|name| CardName(name.clone())),
        ..SubjectFilter::default()
    };
    Ok(EventPattern::Evolve {
        binding: event,
        predicate: filter.predicate(SubjectFields::Evolve(event)),
    })
}
