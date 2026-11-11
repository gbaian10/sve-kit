use super::*;
use crate::catalog::Catalog;
use crate::game::internal::{CardPlay, InstructionOutcome};
use crate::ir::*;
use alloc::sync::Arc;
use serde_json::{Value, json};

fn paused_after_ex_entry() -> Game {
    let snapshot = json!({"number":"unit","faces":[{"name":"unit","card_class":"ニュートラル","card_type":"フォロワー","cost":"4","power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string();
    let registry = json!({"format":1_u8,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1_u8,"kind":"effect_set","cards":{"unit":{"status":"complete","review":"synthetic","abilities":[]}}}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let initial = json!({"history":"explicit","turn":{"active":"P1","first_player":"P1","phase":"main","elapsed_turns":{"P1":3_u8,"P2":2_u8}},"players":{"P1":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":2_u8,"max":2_u8},"ep":0_u8,"sep":0_u8,"zones":{"field":[{"id":"s","card":"unit"}],"deck":[{"id":"arrived","card":"unit"},{"id":"tail","card":"unit"}]}},"P2":{"leader":{"class":"ニュートラル","life":20_u8},"pp":{"current":2_u8,"max":2_u8},"ep":0_u8,"sep":0_u8,"zones":{"field":[{"id":"opposer","card":"unit"}]}}}});
    let mut board = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "followed-current-member",
    )
    .unwrap();
    board.load_internal_card("unit", &paused_program()).unwrap();
    assert!(matches!(
        board
            .activate_internal_card("s", AbilityId(1), PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Awaiting(_)
    ));
    board
}

fn paused_actions() -> Vec<Action> {
    vec![
        Action::Move {
            objects: Selector::Ordered {
                zone: ZoneRef {
                    player: PlayerRef::Controller {},
                    kind: ZoneKind::Deck,
                },
                start: Position::Top {},
                count: Expr::Literal(Nat(1)),
                direction: SliceDirection::Forward,
            },
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Ex,
                },
                position: None,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        },
        Action::Look {
            viewer: PlayerRef::Controller {},
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(1)),
            mode: StepwiseMode::Exact,
        },
        Action::Order {
            decision: DecisionId(3),
            cards: Selector::ReceiptMembers {
                receipt: ObjectReceipt::Look {
                    receipt: ReceiptReference::new(ReceiptId(2)),
                },
                projection: MemberProjection::Before,
            },
            destinations: vec![Placement {
                cards: Selector::ReceiptMembers {
                    receipt: ObjectReceipt::Look {
                        receipt: ReceiptReference::new(ReceiptId(2)),
                    },
                    projection: MemberProjection::Before,
                },
                zone: MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                },
                position: Position::Bottom {},
            }],
            chooser: PlayerRef::Controller {},
            visibility: Visibility::Only {
                players: PlayerSet {
                    relation: PlayerRelation::SelfPlayer,
                    relative_to: PlayerRef::Controller {},
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    text_quantifier: TextQuantifier::One,
                },
            },
        },
    ]
}

fn paused_program() -> WellFormedCardProgram {
    let source = SourceSpan {
        card_id: "unit".into(),
        card_no: "unit".into(),
        scope: "synthetic".into(),
        owner: "card".into(),
        face: "0".into(),
        field: "text".into(),
        section: None,
        line: 1,
        fragment: None,
        author_path: "/".into(),
    };
    let origin = AbilitySource {
        holder: ObjectOperand::SourceHolder {},
        origin: AbilityOrigin::Printed {
            face: FaceId("0".into()),
            line: Nat(1),
        },
        controller: PlayerRef::Controller {},
    };
    let steps = paused_actions()
        .into_iter()
        .enumerate()
        .map(|(i, action)| {
            let id = u64::try_from(i).unwrap().checked_add(1).unwrap();
            Located {
                source: source.clone(),
                node: Node::Instruction {
                    instruction: Box::new(Instruction {
                        id: InstructionId(id),
                        actor: PlayerRef::Controller {},
                        source: origin.clone(),
                        precondition: None,
                        context: InstructionContext::Effect,
                        receipt: ReceiptId(id),
                        action,
                    }),
                },
            }
        })
        .collect();
    WellFormedCardProgram::check(CardProgram {
        abilities: vec![Located {
            source,
            node: Ability {
                id: AbilityId(1),
                scope: ScopeId(1),
                source: origin,
                kind: AbilityKind::Activated,
                qualification: AbilityQualification::Ordinary,
                active_zones: vec![ZoneKind::Field],
                event_captures: vec![],
                play: Some(PlaySpec {
                    limit: None,
                    captures: vec![],
                    parameters: vec![],
                    costs: CostSpec::Ordered { steps: vec![] },
                }),
                trigger: None,
                body: Some(Program {
                    scope: ScopeId(1),
                    steps,
                }),
                declarations: vec![],
            },
        }],
        card_play_costs: vec![],
    })
    .unwrap()
}

#[test]
fn followed_sum_uses_a_real_receipt_and_rejects_redirected_stale_or_displaced_members() {
    let board = paused_after_ex_entry();
    let objects = Selector::MoveAfter {
        receipt: MovementReceipt::Move {
            receipt: ReceiptReference::new(ReceiptId(1)),
        },
        permission: FollowPermission::SameEffectExplicitReference {},
    };
    let execution = board.typed.as_ref().unwrap();
    assert_eq!(
        followed_sum(execution, &board, &objects, NumericProperty::OriginalCost).unwrap(),
        4_i64
    );
    for requested in [false, true] {
        let mut changed = execution.clone();
        let Some(InstructionOutcome::Move(outcome)) = changed.receipts.get_mut(&ReceiptId(1))
        else {
            panic!("the actual completed move owns its receipt")
        };
        if requested {
            outcome.moved[0].requested_to.kind = ZoneKind::Field;
        } else {
            outcome.moved[0].actual_to.as_mut().unwrap().kind = ZoneKind::Field;
        }
        followed_sum(&changed, &board, &objects, NumericProperty::OriginalCost).unwrap_err();
    }
    let mut stale = board.clone();
    stale.object_mut("arrived").unwrap().generation += 1;
    followed_sum(execution, &stale, &objects, NumericProperty::OriginalCost).unwrap_err();
    let mut displaced = board.clone();
    displaced.object_mut("arrived").unwrap().zone = "cemetery".into();
    followed_sum(
        execution,
        &displaced,
        &objects,
        NumericProperty::OriginalCost,
    )
    .unwrap_err();
    let mut empty = execution.clone();
    empty.receipts.insert(
        ReceiptId(1),
        InstructionOutcome::Move(MoveOutcome { moved: vec![] }),
    );
    assert_eq!(
        followed_sum(&empty, &board, &objects, NumericProperty::OriginalCost).unwrap(),
        0_i64
    );
}

#[test]
fn followed_life_reads_current_continuous_changes_from_either_players_context() {
    for seat in [1_u32, 2] {
        for delta in [2_i64, -1, -4] {
            let mut board = paused_after_ex_entry();
            let mut execution = board.typed.as_ref().unwrap().clone();
            let before = execution.reference(&board, "arrived").unwrap();
            let (moved, _) = board
                .internal_move_members(
                    &mut execution,
                    &[before],
                    "field",
                    ZoneKind::Field,
                    MoveMethod::Put,
                    false,
                    None,
                    None,
                )
                .unwrap();
            execution.receipts.insert(
                ReceiptId(4),
                InstructionOutcome::Move(MoveOutcome { moved }),
            );
            let objects = Selector::MoveAfter {
                receipt: MovementReceipt::Move {
                    receipt: ReceiptReference::new(ReceiptId(4)),
                },
                permission: FollowPermission::SameEffectExplicitReference {},
            };
            let mut modifier = execution.clone();
            modifier.context.controller = PlayerId { seat };
            modifier.context.source = if seat == 1 { "s" } else { "opposer" }.into();
            let affected = Binding::new(90, ScopeId(90));
            let duration = Duration {
                end: DurationEnd::NoTemporalExpiry {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            };
            let source = AbilitySource {
                holder: ObjectOperand::SourceHolder {},
                origin: AbilityOrigin::Printed {
                    face: FaceId("0".into()),
                    line: Nat(1),
                },
                controller: PlayerRef::Controller {},
            };
            let instruction = Instruction {
                id: InstructionId(90),
                actor: PlayerRef::Controller {},
                source: source.clone(),
                precondition: None,
                context: InstructionContext::Effect,
                receipt: ReceiptId(90),
                action: Action::InstallEffect {
                    subjects: InstallationSubjects::Objects {
                        objects: objects.clone(),
                        affected,
                    },
                    capture: InstallationTime::InstructionStart,
                    effect: Box::new(Effect::Continuous {
                        id: EffectDefinitionId(90),
                        source,
                        target: EffectTarget::Dynamic {
                            selector: Selector::Bound {
                                reference: affected,
                            },
                        },
                        condition: None,
                        patch: CharacteristicPatch::Numeric {
                            stat: Stat::Life,
                            change: NumericChange::Add {
                                amount: Expr::Literal(Int(delta)),
                            },
                        },
                        layer: ContinuousLayer::Numeric,
                        timestamp: TimestampBasis::ProperPlay {
                            execution: ExecutionRef::Current {},
                        },
                        active_zones: vec![],
                        duration: duration.clone(),
                    }),
                    duration,
                },
            };
            board
                .internal_install_continuous(&mut modifier, &instruction)
                .unwrap();
            assert_eq!(
                followed_sum(&execution, &board, &objects, NumericProperty::Life).unwrap(),
                3_i64 + delta
            );
        }
    }
}

#[test]
fn public_look_reads_require_same_resolution_revelation_and_current_generation() {
    let board = paused_after_ex_entry();
    let mut execution = board.typed.as_ref().unwrap().clone();
    let objects = Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look {
            receipt: ReceiptReference::new(ReceiptId(2)),
        },
        projection: MemberProjection::Before,
    };
    let cost = Expr::Compute(Box::new(IntOperation::Sum {
        objects: objects.clone(),
        property: NumericProperty::OriginalCost,
    }));
    let names = Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Compute(Box::new(NatOperation::DistinctCount {
            objects: objects.clone(),
            property: ObjectProperty::Names,
        })),
    }));
    let rejects_reads = |candidate: &Execution, game: &Game| {
        candidate.integer(game, &cost).unwrap_err();
        candidate.integer(game, &names).unwrap_err();
    };
    rejects_reads(&execution, &board);
    execution.objects(&board, &objects).unwrap_err();
    let Some(InstructionOutcome::Look(looked)) = execution.receipts.get(&ReceiptId(2)) else {
        panic!("the paused program owns a private Look receipt")
    };
    let members = looked.looked.iter().map(|view| view.object).collect();
    execution
        .receipts
        .insert(ReceiptId(99), InstructionOutcome::Revealed(members));
    assert_eq!(execution.integer(&board, &cost).unwrap(), Int(4));
    assert_eq!(execution.integer(&board, &names).unwrap(), Int(1));
    let mut stale = board.clone();
    stale.object_mut("tail").unwrap().generation += 1;
    rejects_reads(&execution, &stale);
    execution.objects(&stale, &objects).unwrap_err();
    let mut displaced = board.clone();
    displaced.object_mut("tail").unwrap().zone = "hand".into();
    rejects_reads(&execution, &displaced);
    let mut foreign = execution.clone();
    foreign
        .look_viewers
        .insert(ReceiptId(2), PlayerId { seat: 2 });
    rejects_reads(&foreign, &board);
    execution.receipts.remove(&ReceiptId(99));
    rejects_reads(&execution, &board);
}
