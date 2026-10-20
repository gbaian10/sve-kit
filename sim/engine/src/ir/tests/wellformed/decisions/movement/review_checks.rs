use super::*;

#[test]
fn bp03_002_search_found_cost_is_local_to_the_destination_question() {
    let found = Binding::new(2, ScopeId(3));
    let object = Binding::new(1, ScopeId(4));
    let condition = Expr::Compute(Box::new(Predicate::SearchFoundAny {
        found,
        object,
        predicate: Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                information: NumberRead::Cost {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::Bound { binding: object }),
                    },
                    view: CharacteristicView::Effective,
                },
            })),
            comparison: Comparison::LessEqual,
            right: Expr::Literal(Int(2)),
        })),
    }));
    let mut node = search();
    if let Action::Search { destination, .. } = action(&mut node) {
        *destination = Some(SearchDestination::OptionalSelfReplacement {
            predicate: condition.clone(),
            movement: movement(shared(ZoneKind::Field)),
        });
    }
    accept(body(1, vec![node.clone()]));
    reject(
        body(
            1,
            vec![
                node.clone(),
                control(Control::If {
                    condition,
                    then_body: body(5, vec![]),
                    else_body: None,
                }),
            ],
        ),
        WellFormedReason::UnavailableBinding,
    );
    if let Action::Search {
        destination: Some(SearchDestination::OptionalSelfReplacement { predicate, .. }),
        ..
    } = action(&mut node)
    {
        *predicate = Expr::Compute(Box::new(Predicate::SearchFoundAny {
            found: Binding::new(3, ScopeId(3)),
            object,
            predicate: Expr::Literal(Bool(true)),
        }));
    }
    reject(body(1, vec![node]), WellFormedReason::BindingType);
}

fn reject_action(program: Program, reason: WellFormedReason) {
    let error = reject(program, reason);
    assert!(
        error.ir_path.ends_with("/instruction/action"),
        "{}",
        error.ir_path
    );
}
fn reject_choice(program: Program) {
    let error = reject(program, WellFormedReason::UnsupportedShape);
    assert!(
        error.ir_path.ends_with("/decision/domain/groups/0"),
        "{}",
        error.ir_path
    );
}

fn installed_for(duration: Duration) -> Located<Node> {
    let mut step = install(1, objects());
    if let Action::InstallEffect {
        duration: outer,
        effect,
        ..
    } = action(&mut step)
        && let Effect::CostModifier {
            duration: inner, ..
        } = effect.as_mut()
    {
        *outer = duration.clone();
        *inner = duration;
    }
    step
}

#[test]
fn every_installed_duration_checks_its_references_before_minting_a_certificate() {
    for end in [
        DurationEnd::NoTemporalExpiry {},
        DurationEnd::EndResolution {},
        DurationEnd::NextTurnEnd {
            player: PlayerRef::Controller {},
        },
        DurationEnd::NextPhaseEdge {
            player: PlayerRef::Controller {},
            phase: Phase::Main,
            edge: Edge::Begin,
        },
        DurationEnd::NextTurnEdge {
            player: PlayerRef::Controller {},
            edge: Edge::End,
        },
        DurationEnd::While {
            predicate: Expr::Literal(Bool(false)),
        },
        DurationEnd::SourceZoneExit {
            source: ObjectOperand::SourceHolder {},
            zone: ZoneKind::Field,
        },
    ] {
        accept(body(1, vec![installed_for(Duration { end, ..turn() })]));
    }
    for end in [
        DurationEnd::NextTurnEnd {
            player: PlayerRef::Bound {
                binding: Binding::new(99, ScopeId(1)),
            },
        },
        DurationEnd::NextPhaseEdge {
            player: PlayerRef::Bound {
                binding: Binding::new(99, ScopeId(1)),
            },
            phase: Phase::Main,
            edge: Edge::Begin,
        },
        DurationEnd::NextTurnEdge {
            player: PlayerRef::Bound {
                binding: Binding::new(99, ScopeId(1)),
            },
            edge: Edge::End,
        },
        DurationEnd::While {
            predicate: Expr::Bound(Binding::new(99, ScopeId(1))),
        },
        DurationEnd::SourceZoneExit {
            source: ObjectOperand::Bound {
                binding: Binding::new(99, ScopeId(1)),
            },
            zone: ZoneKind::Field,
        },
    ] {
        reject_action(
            body(1, vec![installed_for(Duration { end, ..turn() })]),
            WellFormedReason::UnavailableBinding,
        );
    }
    reject_action(
        body(
            1,
            vec![installed_for(Duration {
                anchor: TimeAnchor::Bound {
                    binding: Binding::new(99, ScopeId(1)),
                },
                ..turn()
            })],
        ),
        WellFormedReason::UnavailableBinding,
    );
    for duration in [
        Duration {
            usage: Some(UseLimit {
                count: Nat(1),
                scope: UsageScope::PerObject {
                    object: ObjectOperand::SourceHolder {},
                },
            }),
            ..turn()
        },
        Duration {
            intervals: vec![Interval::DuringNextTurn {
                player: PlayerRef::Controller {},
            }],
            ..turn()
        },
    ] {
        accept(body(1, vec![installed_for(duration)]));
    }
}

#[test]
fn unavailable_movement_producers_and_collections_fail_closed() {
    let producers = [
        moved(1),
        MovementReceipt::Search {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        MovementReceipt::Discard {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        MovementReceipt::Destroy {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        MovementReceipt::Banish {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        MovementReceipt::Order {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        MovementReceipt::Transform {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        MovementReceipt::Fusion {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
    ];
    for receipt in producers {
        reject_action(
            body(1, vec![reveal(2, after(receipt.clone()))]),
            WellFormedReason::UnavailableReceipt,
        );
        reject_choice(body(
            1,
            vec![decision(chosen_objects(2, 1, after(receipt)))],
        ));
    }
    let collections = [
        MovementReceiptList::Move {
            receipts: Binding::new(1, ScopeId(1)),
        },
        MovementReceiptList::Search {
            receipts: Binding::new(1, ScopeId(1)),
        },
        MovementReceiptList::Discard {
            receipts: Binding::new(1, ScopeId(1)),
        },
        MovementReceiptList::Destroy {
            receipts: Binding::new(1, ScopeId(1)),
        },
        MovementReceiptList::Banish {
            receipts: Binding::new(1, ScopeId(1)),
        },
        MovementReceiptList::Order {
            receipts: Binding::new(1, ScopeId(1)),
        },
        MovementReceiptList::Transform {
            receipts: Binding::new(1, ScopeId(1)),
        },
        MovementReceiptList::Fusion {
            receipts: Binding::new(1, ScopeId(1)),
        },
    ];
    for receipts in collections {
        let selector = Selector::MoveAfterAll {
            receipts,
            permission: FollowPermission::SameEffectExplicitReference {},
            deduplicate: GenerationDeduplication::ByObjectAndGeneration,
        };
        reject_action(
            body(1, vec![reveal(2, selector.clone())]),
            WellFormedReason::UnavailableBinding,
        );
        reject_choice(body(1, vec![decision(chosen_objects(2, 1, selector))]));
    }
    let trigger = MovementReceipt::TriggerEvent {
        event: MovementEvent::LeaveField {
            event: Binding::new(1, ScopeId(1)),
        },
    };
    reject_action(
        body(1, vec![reveal(2, after(trigger.clone()))]),
        WellFormedReason::UnsupportedShape,
    );
    reject_choice(body(
        1,
        vec![decision(chosen_objects(2, 1, after(trigger)))],
    ));
}

#[test]
fn search_actor_and_installation_subject_regions_are_explicit() {
    let mut mismatched_actor = search();
    if let Node::Instruction { instruction } = &mut mismatched_actor.node {
        instruction.actor = PlayerRef::Active {};
    }
    let error = reject(
        body(1, vec![mismatched_actor]),
        WellFormedReason::UnsupportedShape,
    );
    assert!(error.ir_path.ends_with("/steps/0/instruction"));
    let mut reused_scope = search();
    if let Action::Search { continuation, .. } = action(&mut reused_scope) {
        continuation.found = Binding::new(2, ScopeId(1));
        continuation.revealed = Binding::new(3, ScopeId(1));
    }
    reject_action(
        body(1, vec![reused_scope]),
        WellFormedReason::DuplicateScope,
    );
    let mut installing_in_parent = install(1, objects());
    if let Action::InstallEffect {
        subjects: InstallationSubjects::Objects { affected, .. },
        ..
    } = action(&mut installing_in_parent)
    {
        *affected = Binding::new(1, ScopeId(1));
    }
    reject_action(
        body(1, vec![installing_in_parent]),
        WellFormedReason::DuplicateScope,
    );
    let mut players = install(1, objects());
    if let Action::InstallEffect { subjects, .. } = action(&mut players) {
        *subjects = InstallationSubjects::Players {
            players: controller_set(),
            affected: Binding::new(1, ScopeId(1001)),
        };
    }
    reject_action(body(1, vec![players]), WellFormedReason::UnsupportedShape);
}

fn search_condition(predicate: Expr<Bool>, destination_condition: bool) -> Located<Node> {
    let mut step = search();
    if let Action::Search {
        filter,
        destination,
        ..
    } = action(&mut step)
    {
        if destination_condition {
            *destination = Some(SearchDestination::OptionalSelfReplacement {
                predicate,
                movement: movement(shared(ZoneKind::Ex)),
            });
        } else {
            *filter = predicate;
        }
    }
    step
}
fn exists(objects: Selector) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::Exists { objects }))
}
fn looking(viewer: PlayerRef) -> Located<Node> {
    instruction(
        Action::Look {
            viewer: viewer.clone(),
            zone: ZoneRef {
                player: viewer,
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(1)),
            mode: StepwiseMode::Exact,
        },
        10,
        "synthetic",
    )
}
fn looked() -> Selector {
    Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look {
            receipt: ReceiptReference::new(ReceiptId(10)),
        },
        projection: MemberProjection::Before,
    }
}

#[test]
fn search_conditions_only_observe_the_searchers_private_information() {
    for destination_condition in [false, true] {
        for selector in [hand(), looked()] {
            accept(body(
                1,
                vec![
                    looking(PlayerRef::Controller {}),
                    search_condition(exists(selector), destination_condition),
                ],
            ));
        }
        let mut opponent_hand = hand();
        if let Selector::Zone { players, .. } = &mut opponent_hand {
            players.relation = PlayerRelation::Opponents;
        }
        for hidden in [
            opponent_hand,
            Selector::Ordered {
                zone: ZoneRef {
                    player: PlayerRef::Active {},
                    kind: ZoneKind::Deck,
                },
                start: Position::Top {},
                count: Expr::Literal(Nat(1)),
                direction: SliceDirection::Forward,
            },
            looked(),
        ] {
            let error = reject(
                body(
                    1,
                    vec![
                        looking(PlayerRef::Active {}),
                        search_condition(exists(hidden), destination_condition),
                    ],
                ),
                WellFormedReason::UnsupportedShape,
            );
            assert!(
                error.ir_path.ends_with(if destination_condition {
                    "/instruction/action/destination"
                } else {
                    "/instruction/action/filter"
                }),
                "{}",
                error.ir_path
            );
        }
    }
    // Engine instructions must not inherit the searcher's private-choice restrictions.
    for destination_condition in [false, true] {
        accept(body(
            1,
            vec![
                looking(PlayerRef::Active {}),
                search_condition(Expr::Literal(Bool(true)), destination_condition),
                reveal(20, looked()),
            ],
        ));
    }
}

#[test]
fn search_requires_a_destination_with_enough_partition_capacity() {
    let mut missing = search();
    if let Action::Search { destination, .. } = action(&mut missing) {
        *destination = None;
    }
    let missing_error = reject(body(1, vec![missing]), WellFormedReason::UnsupportedShape);
    assert!(missing_error.ir_path.ends_with("/action/destination"));
    for (maximum, parts, valid) in [
        (Some(3), vec![Some(1)], false),
        (None, vec![Some(3)], false),
        (Some(3), vec![Some(1), Some(2)], true),
        (None, vec![None], true),
        (Some(u64::MAX), vec![Some(u64::MAX), Some(1)], true),
    ] {
        let mut step = search();
        if let Action::Search {
            cardinality,
            destination,
            ..
        } = action(&mut step)
        {
            *cardinality = bounds(0, maximum);
            *destination = Some(SearchDestination::Partition {
                destinations: parts
                    .into_iter()
                    .map(|limit| SearchPartition {
                        zone: MoveDestination::Owners {
                            zone: OwnerZone::Hand,
                        },
                        cardinality: bounds(0, limit),
                    })
                    .collect(),
            });
        }
        if valid {
            accept(body(1, vec![step]));
        } else {
            let error = reject(body(1, vec![step]), WellFormedReason::UnsupportedShape);
            assert!(error.ir_path.ends_with("/action/destination"));
        }
    }
}

#[test]
fn trait_and_name_reads_reject_unvalidated_views_and_bindings() {
    let current = ObjectView::Current {
        object: Box::new(ObjectOperand::Bound {
            binding: Binding::new(1, ScopeId(2)),
        }),
    };
    let lki = ObjectView::LastKnown {
        object: Box::new(ObjectOperand::SourceHolder {}),
        reason: LkiReason::SourceDeparted,
    };
    for view in [CharacteristicView::Original, CharacteristicView::Effective] {
        let predicate = Expr::Compute(Box::new(Predicate::HasTrait {
            traits: Expr::Live(Box::new(TraitsRead::Traits {
                object: current.clone(),
                view,
            })),
            trait_id: TraitId("synthetic".into()),
        }));
        accept(body(1, vec![search_condition(predicate, false)]));
    }
    accept(body(
        1,
        vec![search_condition(
            Expr::Compute(Box::new(Predicate::HasTrait {
                traits: Expr::Literal(vec![]),
                trait_id: TraitId("synthetic".into()),
            })),
            false,
        )],
    ));
    let invalid_traits = [
        Expr::Frozen(Box::new(TraitsFrozen::Snapshot {
            object: Expr::Bound(Binding::new(99, ScopeId(1))),
        })),
        Expr::Live(Box::new(TraitsRead::Traits {
            object: current.clone(),
            view: CharacteristicView::LastKnown,
        })),
        Expr::Live(Box::new(TraitsRead::Traits {
            object: current,
            view: CharacteristicView::ProspectiveEntry,
        })),
        Expr::Live(Box::new(TraitsRead::Traits {
            object: lki.clone(),
            view: CharacteristicView::Effective,
        })),
        Expr::Bound(Binding::new(99, ScopeId(1))),
        Expr::Captured(CaptureRef {
            binding: Binding::new(99, ScopeId(1)),
        }),
    ];
    let mut predicates: Vec<_> = invalid_traits
        .into_iter()
        .map(|traits| Predicate::HasTrait {
            traits,
            trait_id: TraitId("synthetic".into()),
        })
        .collect();
    for names in [
        Expr::Frozen(Box::new(NamesFrozen::Snapshot {
            object: Expr::Bound(Binding::new(99, ScopeId(1))),
        })),
        Expr::Live(Box::new(NamesRead::Names { object: lki })),
        Expr::Bound(Binding::new(99, ScopeId(1))),
        Expr::Captured(CaptureRef {
            binding: Binding::new(99, ScopeId(1)),
        }),
    ] {
        predicates.push(Predicate::NamesIntersect {
            left: names.clone(),
            right: Expr::Literal(vec![]),
        });
        predicates.push(Predicate::NamesIntersect {
            left: Expr::Literal(vec![]),
            right: names,
        });
    }
    for predicate in predicates {
        let error = reject(
            body(
                1,
                vec![search_condition(Expr::Compute(Box::new(predicate)), false)],
            ),
            WellFormedReason::UnsupportedShape,
        );
        assert!(error.ir_path.ends_with("/action/filter"));
    }
}

#[test]
fn unsupported_installed_effects_and_negative_set_are_not_certified() {
    for effect in [
        Effect::RuleModifier {
            modifier: RuleModifier::EvolutionPlayLimit {
                player: PlayerRef::Controller {},
            },
            condition: Expr::Literal(Bool(true)),
            duration: turn(),
            source: ability_source(),
        },
        Effect::CostModifier {
            subjects: Selector::Bound {
                reference: Binding::new(1, ScopeId(1001)),
            },
            change: CostChange::Set {
                amount: Expr::Literal(Int(-1)),
            },
            duration: turn(),
            usage: None,
        },
    ] {
        let mut step = install(1, objects());
        if let Action::InstallEffect {
            effect: installed, ..
        } = action(&mut step)
        {
            **installed = effect;
        }
        let error = reject(body(1, vec![step]), WellFormedReason::UnsupportedShape);
        assert!(error.ir_path.ends_with("/action/effect"));
    }
    let selector = Selector::MoveAfter {
        receipt: moved(1),
        permission: FollowPermission::RuleCarry {
            rule: RuleCarry::ExToResolutionEffect,
        },
    };
    reject_action(
        body(
            1,
            vec![
                moving(1, objects(), shared(ZoneKind::Ex)),
                reveal(2, selector),
            ],
        ),
        WellFormedReason::UnsupportedShape,
    );
}

#[test]
fn movement_collections_keep_discard_destroy_banish_types_and_private_provenance() {
    let cases = [
        (
            Action::Discard {
                player: PlayerRef::Controller {},
                mode: DiscardMode::DownTo { count: Nat(1) },
            },
            ReceiptExport::Discard {
                source: ReceiptReference::new(ReceiptId(1)),
                iteration: Binding::new(1, ScopeId(3)),
                collect: Some(Binding::new(4, ScopeId(1))),
            },
            MovementReceiptList::Discard {
                receipts: Binding::new(4, ScopeId(1)),
            },
            MovementReceipt::Discard {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
        ),
        (
            Action::Destroy { objects: objects() },
            ReceiptExport::Destroy {
                source: ReceiptReference::new(ReceiptId(1)),
                iteration: Binding::new(1, ScopeId(3)),
                collect: Some(Binding::new(4, ScopeId(1))),
            },
            MovementReceiptList::Destroy {
                receipts: Binding::new(4, ScopeId(1)),
            },
            MovementReceipt::Destroy {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
        ),
        (
            Action::Banish { objects: objects() },
            ReceiptExport::Banish {
                source: ReceiptReference::new(ReceiptId(1)),
                iteration: Binding::new(1, ScopeId(3)),
                collect: Some(Binding::new(4, ScopeId(1))),
            },
            MovementReceiptList::Banish {
                receipts: Binding::new(4, ScopeId(1)),
            },
            MovementReceipt::Banish {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
        ),
    ];
    for (producer, export, receipts, receipt) in cases {
        let producer = instruction(producer, 1, "synthetic");
        let repeat = control(Control::Repeat {
            body: RepeatBody::Program {
                body: body(2, vec![producer.clone()]),
            },
            termination: Termination::Count {
                count: Expr::Literal(Nat(2)),
            },
            check: IterationCheck::BeforeIteration,
            scope: ScopeId(3),
            receipts: vec![export],
        });
        let joined = Selector::MoveAfterAll {
            receipts,
            permission: FollowPermission::SameEffectExplicitReference {},
            deduplicate: GenerationDeduplication::ByObjectAndGeneration,
        };
        accept(body(1, vec![repeat.clone(), reveal(2, joined.clone())]));
        for visibility in [
            Visibility::Public {},
            Visibility::Only {
                players: controller_set(),
            },
        ] {
            let mut choice = chosen_objects(8, 1, joined.clone());
            choice.visibility = visibility.clone();
            reject_choice(body(1, vec![repeat.clone(), decision(choice)]));
            let mut before_choice = chosen_objects(
                8,
                1,
                Selector::ReceiptMembers {
                    receipt: ObjectReceipt::Movement {
                        receipt: receipt.clone(),
                    },
                    projection: MemberProjection::Before,
                },
            );
            before_choice.visibility = visibility;
            reject_choice(body(1, vec![producer.clone(), decision(before_choice)]));
        }
    }
}

#[test]
fn installed_cost_set_accepts_nonnegative_values_and_defined_selection() {
    for amount in [0, 3] {
        let mut step = install(1, objects());
        if let Action::InstallEffect { effect, .. } = action(&mut step)
            && let Effect::CostModifier { change, .. } = effect.as_mut()
        {
            *change = CostChange::Set {
                amount: Expr::Literal(Int(amount)),
            };
        }
        accept(body(1, vec![step]));
    }
    let choice = chosen_objects(10, 1, zone(PlayerRef::Controller {}, ZoneKind::Field, 20));
    accept(body(
        1,
        vec![
            decision(choice),
            install(
                1,
                Selector::Bound {
                    reference: Binding::new(10, ScopeId(1)),
                },
            ),
        ],
    ));
}
