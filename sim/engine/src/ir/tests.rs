use super::*;

fn source(card: &str) -> SourceSpan {
    SourceSpan {
        card_id: card.to_owned(),
        card_no: card.to_owned(),
        scope: "ability:1".to_owned(),
        owner: "card".to_owned(),
        face: "base".to_owned(),
        field: "text".to_owned(),
        section: None,
        line: 1,
        fragment: None,
        author_path: "/abilities/0/body".to_owned(),
    }
}
fn ability_source() -> AbilitySource {
    AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin: AbilityOrigin::Printed {
            face: FaceId("base".to_owned()),
            line: Nat(1),
        },
        controller: PlayerRef::Controller {},
    }
}
fn instruction(action: Action, receipt: u64, card: &str) -> Located<Node> {
    Located {
        source: source(card),
        node: Node::Instruction {
            instruction: Box::new(Instruction {
                id: InstructionId(receipt),
                actor: PlayerRef::Controller {},
                source: ability_source(),
                precondition: None,
                context: InstructionContext::Effect,
                receipt: ReceiptId(receipt),
                action,
            }),
        },
    }
}
fn program(steps: Vec<Located<Node>>) -> Program {
    Program {
        scope: ScopeId(1),
        steps,
    }
}
fn card(body: Program, number: &str) -> Located<CardProgram> {
    Located {
        source: source(number),
        node: CardProgram {
            card_play_costs: vec![],
            abilities: vec![Located {
                source: source(number),
                node: Ability {
                    scope: body.scope,
                    event_captures: vec![],
                    id: AbilityId(1),
                    source: ability_source(),
                    kind: AbilityKind::Spell,
                    qualification: AbilityQualification::Ordinary,
                    active_zones: vec![ZoneKind::Field],
                    play: None,
                    trigger: None,
                    body: Some(body),
                    declarations: vec![],
                },
            }],
        },
    }
}
fn controller_set() -> PlayerSet {
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
fn bp08_body() -> Program {
    let deck = Selector::Zone {
        players: controller_set(),
        zone: ZoneKind::Deck,
        view: SelectionView::Current {},
        // The trigger reserves its subject and facet before lowering the body.
        // Definition validation reserves a scope before the live count is lowered.
        binding: Binding::new(4, ScopeId(5)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    };
    let count = Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation: Arithmetic::Subtract,
        arguments: vec![
            Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Live(Box::new(NatRead::ZoneCount { selector: deck })),
            })),
            Expr::Literal(Int(5)),
        ],
    }));
    program(vec![instruction(
        Action::Draw {
            player: PlayerRef::Controller {},
            count,
            mode: StepwiseMode::Exact,
        },
        1,
        "BP08-038",
    )])
}
fn source_trigger(pattern: EventPattern) -> TriggerSpec {
    TriggerSpec {
        on: TriggerCondition::Event { pattern },
        active_zones: vec![ZoneKind::Field],
        view: EventSnapshotPolicy::AfterEntry,
        limit: None,
        aggregation: TriggerAggregation::PerFact {},
    }
}
fn bp08() -> Located<CardProgram> {
    let mut seed = card(bp08_body(), "BP08-038");
    for ability in &mut seed.node.abilities {
        ability.node.kind = AbilityKind::Triggered;
        ability.node.source.origin = AbilityOrigin::Keyword {
            keyword: KeywordId::Fanfare,
            origin: Box::new(ability.node.source.origin.clone()),
        };
        if let Some(body) = &mut ability.node.body {
            for step in &mut body.steps {
                if let Node::Instruction { instruction } = &mut step.node {
                    instruction.source = ability.node.source.clone();
                }
            }
        }
        let event = Binding::new(1, ScopeId(1));
        ability.node.trigger = Some(source_trigger(EventPattern::EnterField {
            binding: event,
            predicate: Expr::Compute(Box::new(Predicate::ObjectEquals {
                left: Expr::Frozen(Box::new(ObjectFrozen::EnterFieldObject { event })),
                right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
            })),
        }));
    }
    seed
}
fn turn() -> Duration {
    Duration {
        end: DurationEnd::CurrentTurnEnd {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    }
}
fn cp04_filter(candidate: Binding<ObjectRef>) -> Expr<Bool> {
    let candidate_view = ObjectView::Current {
        object: Box::new(ObjectOperand::Bound { binding: candidate }),
    };
    let self_view = ObjectView::Current {
        object: Box::new(ObjectOperand::SourceHolder {}),
    };
    Expr::Compute(Box::new(Predicate::And {
        predicates: vec![
            Expr::Compute(Box::new(Predicate::HasTrait {
                traits: Expr::Live(Box::new(TraitsRead::Traits {
                    object: candidate_view.clone(),
                    view: CharacteristicView::Effective,
                })),
                trait_id: TraitId("ドラゴンズネスト".to_owned()),
            })),
            Expr::Compute(Box::new(Predicate::Not {
                predicate: Expr::Compute(Box::new(Predicate::NamesIntersect {
                    left: Expr::Live(Box::new(NamesRead::Names {
                        object: candidate_view,
                    })),
                    right: Expr::Live(Box::new(NamesRead::Names { object: self_view })),
                })),
            })),
        ],
    }))
}
fn cp04(follow_moved: bool, set: bool) -> Located<CardProgram> {
    let candidate = Binding::new(1, ScopeId(1));
    let filter = cp04_filter(candidate);
    let search = instruction(
        Action::Search {
            decision: DecisionId(9001),
            player: PlayerRef::Controller {},
            deck: PlayerRef::Controller {},
            candidate,
            filter,
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            groups: None,
            destination: Some(SearchDestination::Single {
                movement: MoveSpec {
                    to: MoveDestination::Shared {
                        zone: ZoneRef {
                            player: PlayerRef::Controller {},
                            kind: ZoneKind::Ex,
                        },
                    },
                    initial_posture: None,
                    entry_effects: vec![],
                    position: None,
                    order: None,
                },
            }),
            continuation: SearchContinuation {
                found: Binding::new(2, ScopeId(2)),
                revealed: Binding::new(3, ScopeId(2)),
            },
        },
        1,
        "CP04-060",
    );
    let objects = if follow_moved {
        Selector::MoveAfter {
            receipt: MovementReceipt::Search {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
            permission: FollowPermission::SameEffectExplicitReference {},
        }
    } else {
        // This binding has no producer in the search continuation or outer body.
        Selector::Bound {
            reference: Binding::new(2, ScopeId(1)),
        }
    };
    let affected = Binding::new(4, ScopeId(3));
    let install = instruction(
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::CostModifier {
                kind: CostKind::Play,
                subjects: Selector::Bound {
                    reference: affected,
                },
                change: if set {
                    CostChange::Set {
                        amount: Expr::Literal(Int(0)),
                    }
                } else {
                    CostChange::Delta {
                        amount: Expr::Literal(Int(-2)),
                    }
                },
                duration: turn(),
                usage: None,
            }),
            duration: turn(),
        },
        2,
        "CP04-060",
    );
    let mut seed = card(program(vec![search, install]), "CP04-060");
    for ability in &mut seed.node.abilities {
        ability.node.kind = AbilityKind::Triggered;
        ability.node.qualification = AbilityQualification::UnionBurst;
        ability.node.source.origin = AbilityOrigin::Keyword {
            keyword: KeywordId::OnEvolve,
            origin: Box::new(ability.node.source.origin.clone()),
        };
        let event = Binding::new(1, ScopeId(1));
        ability.node.trigger = Some(source_trigger(EventPattern::Evolve {
            binding: event,
            predicate: Expr::Compute(Box::new(Predicate::ObjectEquals {
                left: Expr::Frozen(Box::new(ObjectFrozen::EvolveSubject { event })),
                right: Expr::Frozen(Box::new(ObjectFrozen::SourceHolder {})),
            })),
        }));
    }
    seed
}
#[test]
fn registry_is_closed_and_versions_do_not_imply_backward_support() {
    let seed = bp08();
    assert!(Registry::current().supported().is_empty());
    assert!(matches!(
        Registry::current().check_capabilities(&seed),
        Err(AdmissionError::Missing(_))
    ));
    let mut registry = Registry::test_support(&seed);
    registry.test_upgrade("action.draw");
    assert!(matches!(
        registry.check_capabilities(&seed),
        Err(AdmissionError::Missing(_))
    ));
    let mut stale = Registry::test_support(&seed);
    stale.dsl = "old";
    stale.ir_schema.hash.push('0');
    stale.derivation.hash.push('0');
    stale.engine_build.push('0');
    let Err(AdmissionError::StaleRegistry(mismatches)) = stale.check_capabilities(&seed) else {
        panic!("stale identities admitted");
    };
    let names: Vec<_> = mismatches.iter().map(|item| item.dependency).collect();
    assert_eq!(
        names,
        vec!["dsl", "ir_schema.hash", "engine_build", "derivation.hash"]
    );
    for mismatch in mismatches {
        assert_ne!(mismatch.expected, mismatch.actual);
    }
}

mod walk;

mod wellformed;

mod lowering;

#[cfg(feature = "runner")]
pub(crate) use lowering::payment_expanded_tree;
