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
        binding: Binding::new(1, ScopeId(1)),
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
                    to: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex,
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
        // An arbitrary bound object set is not evidence of a successful search move.
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
fn codes(program: &Located<CardProgram>) -> Vec<&'static str> {
    Requirements::derive(program)
        .required
        .into_iter()
        .map(|requirement| requirement.capability.code)
        .collect()
}
fn assert_missing(seed: &Located<CardProgram>, code: &str, card: &str) {
    let mut registry = Registry::test_support(seed);
    registry.test_remove(code);
    let Err(AdmissionError::Missing(requirements)) = registry.check_capabilities(seed) else {
        panic!("missing capability admitted");
    };
    assert_eq!(requirements.len(), 1);
    for requirement in requirements {
        assert_eq!(requirement.capability.code, code);
        assert!(!requirement.locations.is_empty());
        for location in requirement.locations {
            assert_eq!(location.source.card_no, card);
            assert!(location.ir_path.starts_with("/abilities/0"));
        }
    }
}
#[test]
fn bp08_preserves_live_signed_draw_without_unsigned_narrowing() {
    let seed = bp08();
    let codes = codes(&seed);
    for code in ["value.live", "value.signed-arithmetic", "action.draw"] {
        assert!(codes.contains(&code));
        assert_missing(&seed, code, "BP08-038");
    }
    assert!(!codes.contains(&"value.checked-nat"));
    assert!(!codes.contains(&"value.capture"));
    assert_eq!(
        Registry::test_support(&seed).check_capabilities(&seed),
        Ok(Requirements::derive(&seed))
    );
}
#[test]
fn negative_draw_preserves_signed_literal_and_capture_is_explicit() {
    let frozen = CaptureRef {
        binding: Binding::new(1, ScopeId(1)),
    };
    let direct = card(
        program(vec![instruction(
            Action::Draw {
                player: PlayerRef::Controller {},
                count: Expr::Literal(Int(-1)),
                mode: StepwiseMode::Exact,
            },
            1,
            "synthetic",
        )]),
        "synthetic",
    );
    let captured = card(
        program(vec![
            Located {
                source: source("synthetic"),
                node: Node::Capture {
                    capture: Capture::Integer {
                        at: CaptureTime::AtPlay {
                            execution: ExecutionRef::Current {},
                        },
                        value: Expr::Literal(Int(-1)),
                        into: frozen,
                    },
                },
            },
            instruction(
                Action::Draw {
                    player: PlayerRef::Controller {},
                    count: Expr::Captured(frozen),
                    mode: StepwiseMode::Exact,
                },
                2,
                "synthetic",
            ),
        ]),
        "synthetic",
    );
    assert!(codes(&direct).contains(&"value.signed-literal"));
    assert!(!codes(&direct).contains(&"value.checked-nat"));
    assert!(codes(&captured).contains(&"value.capture"));
    assert!(!codes(&captured).contains(&"value.live"));
    assert_ne!(
        Requirements::derive(&direct),
        Requirements::derive(&captured)
    );
}
#[test]
fn cp04_preserves_filter_moved_set_installation_delta_and_expiry() {
    let seed = cp04(true, false);
    for code in [
        "read.traits",
        "read.name",
        "predicate.names-intersect",
        "action.search",
        "receipt.move-after",
        "reference.follow-generation",
        "action.install-effect",
        "effect.cost-delta",
        "duration.turn",
    ] {
        assert!(codes(&seed).contains(&code));
        assert_missing(&seed, code, "CP04-060");
    }
    assert_eq!(
        Registry::test_support(&seed).check_capabilities(&seed),
        Ok(Requirements::derive(&seed))
    );
    let wrong_reference = cp04(false, false);
    assert!(!codes(&wrong_reference).contains(&"reference.follow-generation"));
    assert!(matches!(
        Registry::test_support(&wrong_reference).check_capabilities(&seed),
        Err(AdmissionError::Missing(_))
    ));
    let wrong_change = cp04(true, true);
    assert!(codes(&wrong_change).contains(&"effect.cost-set"));
    assert!(!codes(&wrong_change).contains(&"effect.cost-delta"));
    assert!(matches!(
        Registry::test_support(&wrong_change).check_capabilities(&seed),
        Err(AdmissionError::Missing(_))
    ));
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
#[test]
fn false_branches_are_checked_and_all_locations_retained() {
    let seed = card(
        program(vec![Located {
            source: source("BP08-038"),
            node: Node::Control {
                control: Control::If {
                    condition: Expr::Literal(Bool(false)),
                    then_body: bp08_body(),
                    else_body: Some(bp08_body()),
                },
            },
        }]),
        "BP08-038",
    );
    let requirements = Requirements::derive(&seed);
    let draws: Vec<_> = requirements
        .required
        .iter()
        .filter(|item| item.capability.code == "action.draw")
        .collect();
    assert_eq!(draws.len(), 1);
    for draw in draws {
        assert_eq!(draw.locations.len(), 2);
    }
    assert_eq!(requirements, Requirements::derive(&seed));
    let Err(AdmissionError::Missing(missing)) = Registry::current().check_capabilities(&seed)
    else {
        panic!("unsupported branch admitted");
    };
    assert_eq!(missing, requirements.required);
}
#[test]
fn static_declarations_and_play_costs_are_not_omitted() {
    let mut seed = card(program(vec![]), "synthetic");
    for ability in &mut seed.node.abilities {
        ability.node.declarations.push(Effect::CostModifier {
            subjects: Selector::Objects { objects: vec![] },
            change: CostChange::Set {
                amount: Expr::Literal(Int(0)),
            },
            duration: turn(),
            usage: None,
        });
        ability.node.play = Some(PlaySpec {
            captures: vec![],
            parameters: vec![],
            costs: CostSpec::Ordered {
                steps: vec![CostStep {
                    instruction: Box::new(Instruction {
                        id: InstructionId(9),
                        actor: PlayerRef::Controller {},
                        source: ability_source(),
                        precondition: None,
                        context: InstructionContext::Cost,
                        receipt: ReceiptId(9),
                        action: Action::Discard {
                            player: PlayerRef::Controller {},
                            mode: DiscardMode::DownTo { count: Nat(1) },
                        },
                    }),
                    materials: None,
                }],
            },
        });
    }
    assert_missing(&seed, "effect.cost-set", "synthetic");
    assert_missing(&seed, "action.discard", "synthetic");
}
#[test]
fn distinct_patch_shapes_cannot_share_a_support_declaration() {
    let patch = |patch| {
        card(
            program(vec![instruction(
                Action::Modify {
                    subjects: Selector::Objects { objects: vec![] },
                    patch,
                    expires: turn(),
                    source: EffectReference::CurrentInstallation {},
                },
                1,
                "synthetic",
            )]),
            "synthetic",
        )
    };
    let add_trait = patch(CharacteristicPatch::AddTrait {
        trait_id: TraitId("synthetic".to_owned()),
    });
    let remove_trait = patch(CharacteristicPatch::RemoveTrait {
        trait_id: TraitId("synthetic".to_owned()),
    });
    let add_name = patch(CharacteristicPatch::AddName {
        name: CardName("synthetic".to_owned()),
    });
    let support = Registry::test_support(&add_trait);
    assert!(matches!(
        support.check_capabilities(&remove_trait),
        Err(AdmissionError::Missing(_))
    ));
    assert!(matches!(
        support.check_capabilities(&add_name),
        Err(AdmissionError::Missing(_))
    ));
}

#[test]
fn narrowed_payment_history_requires_its_own_projection_and_aggregate() {
    let event = Binding::new(1, ScopeId(7));
    let component = Binding::new(2, ScopeId(7));
    let paid = Expr::Compute(Box::new(Predicate::CompareNat {
        left: Expr::Frozen(Box::new(NatFrozen::EarthRiteRemovedStack { component })),
        comparison: Comparison::Greater,
        right: Expr::Literal(Nat(0)),
    }));
    let mut seed = card(
        program(vec![instruction(
            Action::Draw {
                player: PlayerRef::Controller {},
                count: Expr::Live(Box::new(IntRead::HistorySum {
                    query: NumericHistoryQuery::EarthRiteRemovedStack {
                        event,
                        component,
                        predicate: paid.clone(),
                        window: HistoryWindow {
                            range: HistoryRange::CurrentTurn {},
                            cutoff: HistoryCutoff::CurrentPoint {},
                            relative_player: None,
                        },
                        include_execution: InclusionPolicy::IncludeAll {},
                    },
                })),
                mode: StepwiseMode::Exact,
            },
            1,
            "synthetic",
        )]),
        "synthetic",
    );
    for ability in &mut seed.node.abilities {
        ability.node.trigger = Some(source_trigger(EventPattern::AnyOf {
            patterns: vec![EventPattern::EarthRitePayment {
                event,
                component,
                predicate: paid.clone(),
            }],
        }));
    }
    for required in [
        "event.payment.earth-rite",
        "read.payment.earth-rite-stack",
        "history.sum.earth-rite-stack",
        "event.any-of",
    ] {
        assert_missing(&seed, required, "synthetic");
    }
    let requirements = Requirements::derive(&seed);
    for requirement in &requirements.required {
        if requirement.capability.code == "read.payment.earth-rite-stack" {
            assert_eq!(requirement.locations.len(), 2);
            assert!(requirement.locations.iter().all(|location| {
                location.ir_path.contains("/frozen") && !location.ir_path.contains("/frozen/live")
            }));
        }
    }
}

#[test]
fn fixed_literal_support_does_not_admit_a_runtime_binding() {
    let draw = |count| {
        card(
            program(vec![instruction(
                Action::Draw {
                    player: PlayerRef::Controller {},
                    count,
                    mode: StepwiseMode::Exact,
                },
                1,
                "synthetic",
            )]),
            "synthetic",
        )
    };
    let literal = draw(Expr::Literal(Int(1)));
    let bound = draw(Expr::Bound(Binding::new(1, ScopeId(1))));
    let Err(AdmissionError::Missing(missing)) =
        Registry::test_support(&literal).check_capabilities(&bound)
    else {
        panic!("fixed-count support admitted a runtime binding");
    };
    assert_eq!(missing.len(), 1);
    for requirement in missing {
        assert_eq!(requirement.capability.code, "shape.Expr.Bound");
    }
}

mod events;

mod flow;

mod composition;

mod wellformed;
