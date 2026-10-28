#![expect(
    clippy::panic,
    reason = "A different typed outcome or node fails the fixture immediately."
)]

use super::*;

fn destroy(id: u64, objects: Selector) -> Located<Node> {
    instruction(id, Action::Destroy { objects })
}
pub(super) fn put(id: u64, objects: Selector, zone: OwnerZone) -> Located<Node> {
    instruction(
        id,
        Action::Move {
            objects,
            movement: MoveSpec {
                to: MoveDestination::Owners { zone },
                initial_posture: None,
                entry_effects: vec![],
                position: None,
                order: None,
            },
            reason: MoveReason::Put,
        },
    )
}
pub(super) fn protection(id: u64, objects: Selector, cause: CausePattern) -> Located<Node> {
    let affected = Binding::new(1, ScopeId(100_u64.checked_add(id).unwrap()));
    let targets = Selector::Bound {
        reference: affected,
    };
    let duration = Duration {
        end: DurationEnd::EndResolution {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    instruction(
        id,
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects { objects, affected },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::Permission {
                subject: PermissionSubject::Objects {
                    objects: targets.clone(),
                },
                action: ActionPattern::Destroy {
                    objects: targets,
                    cause,
                },
                polarity: Polarity::Forbid,
                condition: Expr::Literal(Bool(true)),
                duration: duration.clone(),
                rule: RuleId(1),
            }),
            duration,
        },
    )
}
pub(super) const fn effect_cause() -> CausePattern {
    CausePattern::Effect {
        source: EffectSource::Any {},
    }
}
fn capture_objects(slot: u64, objects: Selector) -> Located<Node> {
    node(Node::Capture {
        capture: Capture::Objects {
            at: CaptureTime::AtResolution { scope: ScopeId(1) },
            value: objects,
            into: CaptureRef {
                binding: Binding::new(slot, ScopeId(1)),
            },
        },
    })
}
const fn captured(slot: u64) -> Selector {
    Selector::Captured {
        reference: CaptureRef {
            binding: Binding::new(slot, ScopeId(1)),
        },
    }
}
fn destroy_outcome(progress: &Progress, id: u64) -> &DestroyOutcome {
    let Some(InstructionOutcome::Destroy(outcome)) = progress.receipts.get(&ReceiptId(id)) else {
        panic!("missing destruction outcome")
    };
    outcome
}

fn field_followers(zone: ZoneKind) -> Selector {
    let binding = Binding::new(9, ScopeId(1));
    Selector::Zone {
        players: players(PlayerRelation::Opponents),
        zone,
        view: SelectionView::Current {},
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::And {
            predicates: vec![Expr::Compute(Box::new(Predicate::KindEquals {
                left: Expr::Live(Box::new(KindRead::Kind {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::Bound { binding }),
                    },
                    view: CharacteristicView::Effective,
                })),
                right: Expr::Literal(CardKind::Follower),
            }))],
        }))),
    }
}

#[test]
fn synthetic_public_field_destroy_filters_each_candidate_and_keeps_amulets() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        let mut position = setup(seat, json!([]));
        position["players"][actor]["zones"]["field"] = json!([{"id":"own","card":"unit-follower"}]);
        position["players"][opponent]["zones"]["field"] = json!([
            {"id":"f","card":"unit-follower"}, {"id":"a","card":"unit-amulet"}
        ]);
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "field-filter",
        )
        .unwrap();
        let result = game
            .execute_internal(
                &checked(vec![destroy(1, field_followers(ZoneKind::Field))]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        assert_eq!(destroy_outcome(&result, 1).successful.len(), 1);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state[opponent]["field"], json!(["a"]));
        assert_eq!(state[actor]["field"], json!(["own"]));
        assert_eq!(state[opponent]["cemetery"], json!(["f"]));
    }
}

#[test]
fn public_field_destroy_does_not_authorize_hidden_or_other_candidate_reads() {
    let mut hidden = field_followers(ZoneKind::Deck);
    let mut name = field_followers(ZoneKind::Field);
    if let Selector::Zone { predicate, .. } = &mut name {
        **predicate = Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
            })),
            right: Expr::Literal(vec![CardName("unit-follower".into())]),
        }));
    }
    if let Selector::Zone { players, .. } = &mut hidden {
        players.relation = PlayerRelation::SelfPlayer;
    }
    for selector in [hidden, name] {
        let mut game = game(1, 1);
        let before = game.projection(View::Referee).unwrap();
        game.execute_internal(
            &checked(vec![destroy(1, selector)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}

#[test]
fn public_field_buff_does_not_authorize_hidden_or_other_candidate_reads() {
    let mut hidden = field_followers(ZoneKind::Deck);
    if let Selector::Zone {
        players,
        binding,
        predicate,
        ..
    } = &mut hidden
    {
        players.relation = PlayerRelation::SelfPlayer;
        *binding = Binding::new(10, ScopeId(1));
        **predicate = Expr::Literal(Bool(true));
    }
    let mut name = field_followers(ZoneKind::Field);
    if let Selector::Zone { predicate, .. } = &mut name {
        **predicate = Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
            })),
            right: Expr::Literal(vec![CardName("unit-follower".into())]),
        }));
    }
    let hidden_exclusion = Selector::Difference {
        input: Box::new(field_followers(ZoneKind::Field)),
        except: Box::new(hidden.clone()),
    };
    let hidden_amount = CharacteristicPatch::Numeric {
        stat: Stat::Attack,
        change: NumericChange::Add {
            amount: Expr::Compute(Box::new(IntOperation::FromNat {
                value: Expr::Compute(Box::new(NatOperation::Count {
                    objects: hidden.clone(),
                })),
            })),
        },
    };
    for (selector, patch) in [
        (hidden, numeric::pair(1, 1)),
        (name, numeric::pair(1, 1)),
        (hidden_exclusion, numeric::pair(1, 1)),
        (field_followers(ZoneKind::Field), hidden_amount),
    ] {
        let mut game = game(1, 1);
        let before = game.projection(View::Referee).unwrap();
        let error = game
            .execute_internal(
                &checked(vec![numeric::modify(
                    1,
                    selector,
                    patch,
                    DurationEnd::NoTemporalExpiry {},
                )]),
                &source("synthetic"),
                context(1),
            )
            .unwrap_err();
        assert!(
            matches!(error, sve_engine::EngineFailure::Unsupported(reason)
            if reason.contains("private observation outside an authorized selector spine")),
            "public field subjects do not authorize private reads elsewhere"
        );
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}

#[test]
fn field_destroy_keeps_candidate_independent_false_conditions() {
    let mut selector = zone(ZoneKind::Field, 1);
    if let Selector::Zone { predicate, .. } = &mut selector {
        **predicate = Expr::Literal(Bool(false));
    }
    let mut game = game(1, 0);
    let result = game
        .execute_internal(
            &checked(vec![destroy(1, selector)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert_eq!(
        destroy_outcome(&result, 1).successful,
        Vec::<ResultMember>::new()
    );
}

#[test]
fn mixed_owners_return_to_their_own_cemeteries_with_independent_generations() {
    for seat in [1, 2] {
        let mut position = setup(seat, json!([]));
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        position["players"][actor]["zones"]["field"] = json!([
            {"id":"a","card":"unit-follower","owner":opponent},
            {"id":"b","card":"unit-follower","owner":actor}
        ]);
        let mut game =
            Game::new(catalog(), &position, &Value::Null, &Value::Null, "mixed").unwrap();
        let selected = Selector::Union {
            sets: vec![zone(ZoneKind::Field, 2), zone(ZoneKind::Field, 3)],
        };
        let result = game
            .execute_internal(
                &checked(vec![destroy(1, selected)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        let outcome = destroy_outcome(&result, 1);
        assert_eq!(outcome.successful.len(), 3);
        assert_eq!(outcome.moves.len(), 3);
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection[actor]["cemetery"].as_array().unwrap().len(), 1);
        assert_eq!(
            projection[opponent]["cemetery"].as_array().unwrap().len(),
            2
        );
        for item in &outcome.moves {
            assert_eq!(
                item.after.unwrap().generation.0,
                item.before.generation.0 + 1
            );
            assert_eq!(item.requested_to, item.actual_to.unwrap());
            assert_eq!(item.requested_position, None);
            assert_eq!(item.actual_position, None);
            assert_eq!(item.replacements.len(), 0);
        }
        for fact in &result.facts {
            let Fact::Movement(fact) = fact else {
                panic!("movement fact")
            };
            assert_eq!(fact.method, MoveMethod::Destroy);
            assert!(matches!(fact.cause, Cause::Effect { .. }));
            assert_eq!(fact.from.kind, ZoneKind::Field);
            assert_eq!(fact.from.player, fact.controller_before);
        }
        assert_ne!(outcome.moves[0].before.id, outcome.moves[1].before.id);
    }
}

#[test]
fn put_to_cemetery_is_not_destruction_and_same_zone_is_not_a_move() {
    let mut game = game(1, 0);
    let result = game
        .execute_internal(
            &checked(vec![
                put(1, zone(ZoneKind::Field, 2), OwnerZone::Cemetery),
                put(2, zone(ZoneKind::Cemetery, 3), OwnerZone::Cemetery),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    let Some(InstructionOutcome::Move(first)) = result.receipts.get(&ReceiptId(1)) else {
        panic!("move")
    };
    let Some(InstructionOutcome::Move(second)) = result.receipts.get(&ReceiptId(2)) else {
        panic!("move")
    };
    assert!(first.moved[0].after.is_some());
    assert!(second.moved[0].after.is_none());
    assert!(second.moved[0].actual_to.is_none());
    assert_eq!(result.facts.len(), 1);
    let Fact::Movement(fact) = &result.facts[0] else {
        panic!("movement")
    };
    assert_eq!(fact.method, MoveMethod::Put);
    let saved = serde_json::to_value(&game).unwrap();
    assert_eq!(saved["state"]["objects"]["f"]["generation"], 1);
}

#[test]
fn protection_blocks_only_destroy_and_expires_at_resolution_end() {
    for seat in [1, 2] {
        let mut game = game(seat, 0);
        let result = game
            .execute_internal(
                &checked(vec![
                    protection(1, zone(ZoneKind::Field, 2), effect_cause()),
                    destroy(2, zone(ZoneKind::Field, 3)),
                ]),
                &source("BP02-089-predicate-fragment"),
                context(seat),
            )
            .unwrap();
        let blocked = destroy_outcome(&result, 2);
        assert!(blocked.successful.is_empty() && result.facts.is_empty());
        assert_eq!(blocked.moves.len(), 1);
        assert!(blocked.moves[0].after.is_none() && blocked.moves[0].actual_to.is_none());
        let result = game
            .execute_internal(
                &checked(vec![destroy(1, zone(ZoneKind::Field, 4))]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        assert_eq!(destroy_outcome(&result, 1).successful.len(), 1);
    }
    for destination in [OwnerZone::Cemetery, OwnerZone::Banish] {
        let mut game = game(1, 0);
        let result = game
            .execute_internal(
                &checked(vec![
                    protection(1, zone(ZoneKind::Field, 2), effect_cause()),
                    put(2, zone(ZoneKind::Field, 3), destination),
                ]),
                &source("synthetic"),
                context(1),
            )
            .unwrap();
        assert_eq!(result.facts.len(), 1);
    }
}

#[test]
fn stale_captures_do_not_destroy_a_later_generation_or_a_same_name_copy() {
    let mut game = game(1, 0);
    let result = game
        .execute_internal(
            &checked(vec![
                capture_objects(1, zone(ZoneKind::Field, 2)),
                put(1, captured(1), OwnerZone::Cemetery),
                destroy(2, captured(1)),
                put(3, zone(ZoneKind::Cemetery, 3), OwnerZone::Banish),
                destroy(4, captured(1)),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert_eq!(destroy_outcome(&result, 2).successful.len(), 0);
    assert_eq!(destroy_outcome(&result, 4).successful.len(), 0);
    assert_eq!(result.facts.len(), 2);
    let saved = serde_json::to_value(&game).unwrap();
    assert_eq!(saved["state"]["objects"]["f"]["generation"], 2);
    let Fact::Movement(first) = &result.facts[0] else {
        panic!("move")
    };
    let Fact::Movement(second) = &result.facts[1] else {
        panic!("move")
    };
    assert_eq!(first.movement.after.unwrap(), second.movement.before);
}

#[test]
fn off_field_destroy_and_late_failure_restore_the_entire_game() {
    for (origin, name) in [
        (ZoneKind::Ex, "ex"),
        (ZoneKind::Cemetery, "cemetery"),
        (ZoneKind::Banish, "banish"),
    ] {
        let mut position = setup(1, json!([{"id":"d","card":"unit-follower"}]));
        position["players"]["P2"]["zones"][name] = json!([{"id":"outside","card":"unit-follower"}]);
        let mut game =
            Game::new(catalog(), &position, &Value::Null, &Value::Null, "rollback").unwrap();
        let before = exact_authority(&game);
        let program = checked(vec![
            draw(1, 1),
            put(2, zone(ZoneKind::Field, 2), OwnerZone::Cemetery),
            destroy(3, zone(origin, 3)),
        ]);
        game.execute_internal(&program, &source("synthetic"), context(1))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn movement_receipts_and_new_generations_are_absent_from_player_packets() {
    let mut game = game(1, 2);
    let progress = game
        .execute_internal(
            &checked(vec![destroy(1, zone(ZoneKind::Field, 2)), up_to(2, 1)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert!(!progress.complete);
    assert_eq!(destroy_outcome(&progress, 1).successful.len(), 1);
    for view in [View::P1, View::P2] {
        let packet = game.projection(view).unwrap().to_string();
        for private in [
            "receipts",
            "requested_to",
            "actual_to",
            "replacements",
            "ObjectId",
            "destroy_guards",
        ] {
            assert!(!packet.contains(private));
        }
    }
    let decision = game.internal_decision(View::P1).unwrap();
    let before = exact_authority(&game);
    game.answer_internal(
        PlayerId { seat: 2 },
        decision.token,
        Answer::Stepwise(StepwiseAnswer::Continue),
    )
    .unwrap_err();
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn end_resolution_guard_does_not_prevent_lethal_damage_rule_cleanup() {
    let mut game = game(1, 0);
    let result = game
        .execute_internal(
            &checked(vec![
                protection(1, zone(ZoneKind::Field, 2), effect_cause()),
                damage(
                    2,
                    RecipientSet::Objects {
                        objects: zone(ZoneKind::Field, 3),
                    },
                    3,
                ),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert!(result.complete);
    let projection = game.projection(View::Referee).unwrap();
    assert_eq!(projection["P2"]["field"].as_array().unwrap().len(), 0);
    assert_eq!(projection["P2"]["cemetery"].as_array().unwrap().len(), 1);
}

#[test]
fn late_resume_failure_restores_the_entire_execution_and_guards() {
    let mut game = game(1, 2);
    let origin = game.clone();
    game.execute_internal(
        &checked(vec![
            protection(1, zone(ZoneKind::Field, 2), effect_cause()),
            up_to(2, 1),
            destroy(
                3,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
            ),
        ]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    let decision = game.internal_decision(View::P1).unwrap();
    game.answer_internal(
        PlayerId { seat: 1 },
        decision.token,
        Answer::Stepwise(StepwiseAnswer::Continue),
    )
    .unwrap_err();
    transactions::assert_restored(&game, &origin);
}

#[cfg(feature = "runner")]
#[test]
fn printed_bp02_destroy_and_protection_predicate_fragments_use_public_inputs() {
    use std::{env::var_os, fs::read_to_string, path::PathBuf};
    let snapshot = read_to_string(PathBuf::from(
        var_os("SVE_TEST_SNAPSHOT").expect("generate public card input first"),
    ))
    .unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let fragments = json!({"format":1,"kind":"effect_set","cards":{
        "BP02-116":{"status":"complete","review":"synthetic fragment harness","abilities":[]},
        "BP02-089":{"status":"complete","review":"synthetic fragment harness","abilities":[]}
    }})
    .to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("fragments.yaml".into(), fragments)],
        )
        .unwrap(),
    );
    for seat in [1, 2] {
        for protected in [false, true] {
            let actor = if seat == 1 { "P1" } else { "P2" };
            let opponent = if seat == 1 { "P2" } else { "P1" };
            let mut position = setup(seat, json!([]));
            position["players"][actor]["zones"]["hand"][0]["card"] = json!("BP02-116");
            position["players"][opponent]["zones"]["field"][0]["card"] = json!("BP02-089");
            let mut game = Game::new(
                Arc::clone(&catalog),
                &position,
                &Value::Null,
                &Value::Null,
                "BP02-fragments",
            )
            .unwrap();
            let mut steps = Vec::new();
            if protected {
                steps.push(protection(1, zone(ZoneKind::Field, 2), effect_cause()));
            }
            steps.push(destroy(2, zone(ZoneKind::Field, 3)));
            let result = game
                .execute_internal(
                    &checked(steps),
                    &source("BP02-116-destroy-fragment"),
                    context(seat),
                )
                .unwrap();
            assert_eq!(
                destroy_outcome(&result, 2).successful.len(),
                usize::from(!protected)
            );
            assert_eq!(result.facts.len(), usize::from(!protected));
        }
    }
}

#[test]
fn protection_is_per_generation_and_partial_success_keeps_before_controller() {
    let mut position = setup(1, json!([]));
    position["players"]["P1"]["zones"]["hand"] = json!([]);
    position["players"]["P1"]["zones"]["field"] =
        json!([{"id":"s","card":"unit-follower","owner":"P2"}]);
    let mut game = Game::new(catalog(), &position, &Value::Null, &Value::Null, "partial").unwrap();
    let protected = Selector::Objects {
        objects: vec![ObjectOperand::SourceHolder {}],
    };
    let result = game
        .execute_internal(
            &checked(vec![
                protection(1, protected, effect_cause()),
                up_to(2, 1),
                destroy(3, zone(ZoneKind::Field, 2)),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert!(!result.complete);
    let decision = game.internal_decision(View::P1).unwrap();
    let result = game
        .answer_internal(
            PlayerId { seat: 1 },
            decision.token,
            Answer::Stepwise(StepwiseAnswer::Stop),
        )
        .unwrap();
    let outcome = destroy_outcome(&result, 3);
    assert_eq!(outcome.moves.len(), 2);
    assert_eq!(outcome.successful.len(), 1);
    assert!(outcome.moves[0].after.is_none());
    assert!(outcome.moves[1].after.is_some());
    assert_ne!(
        outcome.successful[0].before.unwrap().snapshot,
        outcome.successful[0].after.unwrap().snapshot
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["field"],
        json!(["s"])
    );
}

#[test]
fn exhausted_generation_cannot_be_reused_and_failed_move_is_atomic() {
    let game = game(1, 2);
    let mut saved = serde_json::to_value(&game).unwrap();
    saved["state"]["objects"]["f"]["generation"] = json!(u64::MAX);
    let mut game: Game = serde_json::from_value(saved).unwrap();
    let before = exact_authority(&game);
    game.execute_internal(
        &checked(vec![draw(1, 1), destroy(2, zone(ZoneKind::Field, 2))]),
        &source("synthetic"),
        context(1),
    )
    .unwrap_err();
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn token_advance_and_linked_objects_reject_the_whole_movement_atomically() {
    let cases = [
        ("f", "card_type", json!("フォロワー・トークン")),
        ("f", "card_type", json!("アドバンス")),
        ("f", "evolved_with", json!("d1")),
        ("f", "links", json!(["d1"])),
        ("s", "equipped_to", json!("f")),
    ];
    let control = game(1, 2)
        .execute_internal(
            &checked(vec![draw(1, 1), destroy(2, zone(ZoneKind::Field, 2))]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert_eq!(destroy_outcome(&control, 2).successful.len(), 1);
    for (holder, field, value) in cases {
        for destroy_it in [true, false] {
            let mut saved = serde_json::to_value(game(1, 2)).unwrap();
            saved["state"]["objects"][holder]["state"][field] = value.clone();
            let mut game: Game = serde_json::from_value(saved).unwrap();
            let before = exact_authority(&game);
            let action = if destroy_it {
                destroy(2, zone(ZoneKind::Field, 2))
            } else {
                put(2, zone(ZoneKind::Field, 2), OwnerZone::Banish)
            };
            game.execute_internal(
                &checked(vec![draw(1, 1), action]),
                &source("synthetic"),
                context(1),
            )
            .unwrap_err();
            assert_eq!(exact_authority(&game), before, "{holder}.{field}");
        }
    }
}

#[test]
fn public_field_destroy_accepts_its_candidate_name_and_preserves_nonmatches() {
    let mut selector = field_followers(ZoneKind::Field);
    if let Selector::Zone {
        predicate, binding, ..
    } = &mut selector
    {
        **predicate = Expr::Compute(Box::new(Predicate::NamesIntersect {
            left: Expr::Live(Box::new(NamesRead::Names {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::Bound { binding: *binding }),
                },
            })),
            right: Expr::Literal(vec![CardName("unit-follower".into())]),
        }));
    }
    let mut initial = setup(1, json!([]));
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"unit-follower"},
        {"id":"amulet","card":"unit-amulet"}
    ]);
    let mut game = Game::new(
        catalog(),
        &initial,
        &Value::Null,
        &Value::Null,
        "named-destroy",
    )
    .unwrap();
    let result = game
        .execute_internal(
            &checked(vec![destroy(1, selector)]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    assert_eq!(destroy_outcome(&result, 1).successful.len(), 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["amulet"]));
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}
