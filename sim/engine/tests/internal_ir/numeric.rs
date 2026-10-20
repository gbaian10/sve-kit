use super::*;
use sve_engine::game::internal::StatChangeFact;

#[path = "numeric_turn.rs"]
mod turn;

const fn patch(stat: Stat, amount: Expr<Int>) -> CharacteristicPatch {
    CharacteristicPatch::Numeric {
        stat,
        change: NumericChange::Add { amount },
    }
}

fn pair(attack: i64, life: i64) -> CharacteristicPatch {
    CharacteristicPatch::Combined {
        patches: vec![
            patch(Stat::Attack, Expr::Literal(Int(attack))),
            patch(Stat::Life, Expr::Literal(Int(life))),
        ],
    }
}

fn modify(
    id: u64,
    subjects: Selector,
    patch: CharacteristicPatch,
    end: DurationEnd,
) -> Located<Node> {
    instruction(
        id,
        Action::Modify {
            subjects,
            patch,
            expires: Duration {
                end,
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            source: EffectReference::CurrentInstallation {},
        },
    )
}

pub(super) fn buff(id: u64, attack: i64, life: i64) -> Located<Node> {
    modify(
        id,
        zone(ZoneKind::Field, 100_u64.saturating_add(id)),
        pair(attack, life),
        DurationEnd::NoTemporalExpiry {},
    )
}

fn changes(progress: &Progress) -> Vec<&StatChangeFact> {
    progress
        .facts
        .iter()
        .filter_map(|fact| match fact {
            Fact::StatChange(fact) => Some(fact),
            Fact::Draw(_)
            | Fact::Damage(_)
            | Fact::Movement(_)
            | Fact::Shuffled { .. }
            | Fact::AbilityPlayed { .. }
            | Fact::AbilityResolved { .. } => None,
        })
        .collect()
}

fn unchanged(game: &mut Game, program: &WellFormedProgram, seat: u32) {
    let before = format!("{game:?}");
    game.execute_internal(program, &source("synthetic"), context(seat))
        .unwrap_err();
    assert_eq!(
        before,
        format!("{game:?}"),
        "failed numeric execution restores complete authority"
    );
}

#[test]
fn combined_add_has_one_fact_per_subject_per_instruction_and_zero_has_none() {
    for seat in [1, 2] {
        let mut game = game(seat, 2);
        let progress = game
            .execute_internal(
                &checked(vec![buff(1, 1, 1), buff(2, -5, 0), buff(3, 0, 0)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        let facts = changes(&progress);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].event.facets.first.property, Stat::Attack);
        assert_eq!(facts[0].event.facets.first.before, Int(2));
        assert_eq!(facts[0].event.facets.first.after, Int(3));
        assert_eq!(facts[0].event.facets.rest.len(), 1);
        assert_eq!(facts[0].event.facets.rest[0].property, Stat::Life);
        assert_eq!(facts[0].event.facets.rest[0].delta, Int(1));
        assert_eq!(facts[1].event.facets.first.delta, Int(-5));
        assert_eq!(facts[1].event.facets.rest.len(), 0);
        assert_ne!(facts[0].event.commit, facts[1].event.commit);
        assert_ne!(facts[0].effect, facts[1].effect);
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection["objects"]["f"]["power"], -2);
        assert_eq!(projection["objects"]["f"]["hp"], 4);
        assert_eq!(projection["objects"]["f"]["max_hp"], 4);
        for view in [View::P1, View::P2] {
            assert_eq!(game.projection(view).unwrap()["objects"]["f"]["power"], -2);
        }
    }
}

#[test]
fn add_preserves_damage_and_rereads_do_not_create_facts() {
    for seat in [1, 2] {
        let mut game = game(seat, 2);
        let progress = game
            .execute_internal(
                &checked(vec![
                    damage(
                        1,
                        RecipientSet::Objects {
                            objects: zone(ZoneKind::Field, 111),
                        },
                        1,
                    ),
                    buff(2, 1, 1),
                    modify(
                        3,
                        zone(ZoneKind::Field, 112),
                        pair(2, 2),
                        DurationEnd::NoTemporalExpiry {},
                    ),
                ]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        let facts = changes(&progress);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].event.facets.rest[0].before, Int(2));
        assert_eq!(facts[0].event.facets.rest[0].after, Int(3));
        assert_eq!(facts[1].event.facets.rest[0].after, Int(5));
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection["objects"]["f"]["power"], 5);
        assert_eq!(projection["objects"]["f"]["hp"], 5);
        assert_eq!(projection["objects"]["f"]["damage"], 1);
        assert_eq!(projection["objects"]["f"]["max_hp"], 6);
        let next = game
            .execute_internal(&checked(vec![]), &source("synthetic"), context(seat))
            .unwrap();
        assert_eq!(changes(&next).len(), 0);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
            5
        );
    }
}

#[test]
fn target_generation_is_fixed_and_source_leaving_does_not_cancel_add() {
    for seat in [1, 2] {
        let mut position = setup(seat, json!([]));
        let actor = if seat == 1 { "P1" } else { "P2" };
        position["players"][actor]["zones"]["hand"] = json!([]);
        position["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-spell"}]);
        let mut game =
            Game::new(catalog(), &position, &Value::Null, &Value::Null, "numeric").unwrap();
        let target = CaptureRef {
            binding: Binding::new(2, ScopeId(1)),
        };
        let program = checked(vec![
            node(Node::Capture {
                capture: Capture::Objects {
                    at: CaptureTime::AtResolution { scope: ScopeId(1) },
                    value: Selector::Zone {
                        players: players(PlayerRelation::Opponents),
                        zone: ZoneKind::Field,
                        view: SelectionView::Current {},
                        binding: Binding::new(9, ScopeId(120)),
                        predicate: Box::new(Expr::Literal(Bool(true))),
                    },
                    into: target.clone(),
                },
            }),
            movement::put(
                1,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Cemetery,
            ),
            modify(
                2,
                Selector::Captured {
                    reference: target.clone(),
                },
                pair(1, 1),
                DurationEnd::NoTemporalExpiry {},
            ),
            movement::put(
                3,
                Selector::Captured {
                    reference: target.clone(),
                },
                OwnerZone::Cemetery,
            ),
            modify(
                4,
                Selector::Captured { reference: target },
                pair(10, 10),
                DurationEnd::NoTemporalExpiry {},
            ),
        ]);
        let progress = game
            .execute_internal(&program, &source("CP02-007"), context(seat))
            .unwrap();
        assert_eq!(changes(&progress).len(), 1);
        assert!(
            matches!(&progress.receipts[&ReceiptId(4)], InstructionOutcome::Modified { effect: None, subjects } if subjects.is_empty())
        );
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection["objects"]["s"]["generation"], 1);
        assert_eq!(projection["objects"]["f"]["generation"], 1);
        assert_eq!(projection["objects"]["f"]["power"], 2);
    }
}

#[test]
fn numeric_effects_survive_save_and_repeated_queries_without_new_events() {
    for seat in [1, 2] {
        let mut game = game(seat, 1);
        game.execute_internal(
            &checked(vec![buff(1, 1, 1)]),
            &source("synthetic"),
            context(seat),
        )
        .unwrap();
        let saved = serde_json::to_value(&game).unwrap();
        let mut restored: Game = serde_json::from_value(saved.clone()).unwrap();
        for _ in 0..3 {
            restored.projection(View::Referee).unwrap();
        }
        assert_eq!(saved, serde_json::to_value(&restored).unwrap());
        let progress = restored
            .execute_internal(
                &checked(vec![buff(2, 2, 2)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        assert_eq!(changes(&progress).len(), 1);
        assert_eq!(changes(&progress)[0].event.facets.first.before, Int(3));
        assert_eq!(
            restored.projection(View::Referee).unwrap()["objects"]["f"]["power"],
            5
        );
    }
}

#[test]
fn numeric_operands_are_frozen_once_before_any_subject_changes() {
    for seat in [1, 2] {
        let mut position = setup(seat, json!([]));
        let actor = if seat == 1 { "P1" } else { "P2" };
        position["players"][actor]["zones"]["hand"] = json!([]);
        position["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
        let mut game =
            Game::new(catalog(), &position, &Value::Null, &Value::Null, "frozen").unwrap();
        let read = Expr::Compute(Box::new(IntOperation::InformationOrZero {
            information: NumberRead::Stats {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                property: Stat::Attack,
                view: CharacteristicView::Effective,
            },
        }));
        let program = checked(vec![modify(
            1,
            zone(ZoneKind::Field, 122),
            CharacteristicPatch::Combined {
                patches: vec![patch(Stat::Attack, read.clone()), patch(Stat::Life, read)],
            },
            DurationEnd::NoTemporalExpiry {},
        )]);
        let progress = game
            .execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        assert_eq!(changes(&progress).len(), 2);
        for id in ["s", "f"] {
            let projection = game.projection(View::Referee).unwrap();
            assert_eq!(projection["objects"][id]["power"], 4);
            assert_eq!(projection["objects"][id]["hp"], 5);
        }
    }
}

#[test]
fn overflow_and_invalid_subject_restore_every_authority_field() {
    for seat in [1, 2] {
        let mut game = game(seat, 2);
        unchanged(
            &mut game,
            &checked(vec![draw(1, 1), buff(2, i64::MAX, 0)]),
            seat,
        );
        unchanged(
            &mut game,
            &checked(vec![
                buff(1, 1, 1),
                modify(
                    2,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    pair(1, 1),
                    DurationEnd::NoTemporalExpiry {},
                ),
            ]),
            seat,
        );
        let mut position = setup(seat, json!([]));
        let opponent = if seat == 1 { "P2" } else { "P1" };
        position["players"][opponent]["zones"]["field"] = json!([]);
        let mut empty =
            Game::new(catalog(), &position, &Value::Null, &Value::Null, "empty").unwrap();
        let progress = empty
            .execute_internal(
                &checked(vec![buff(1, 1, 1)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        assert_eq!(changes(&progress).len(), 0);
        assert!(matches!(
            &progress.receipts[&ReceiptId(1)],
            InstructionOutcome::Modified { effect: None, .. }
        ));
    }
}

#[test]
fn unsupported_numeric_shapes_are_rejected_in_dead_branches_before_draw() {
    for seat in [1, 2] {
        for step in [
            modify(
                2,
                zone(ZoneKind::Field, 130),
                CharacteristicPatch::Numeric {
                    stat: Stat::Attack,
                    change: NumericChange::Set {
                        amount: Expr::Literal(Int(-1)),
                    },
                },
                DurationEnd::NoTemporalExpiry {},
            ),
            modify(
                2,
                zone(ZoneKind::Field, 130),
                pair(1, 1),
                DurationEnd::NextTurnEdge {
                    player: PlayerRef::Controller {},
                    edge: Edge::End,
                },
            ),
        ] {
            let program = checked(vec![
                draw(1, 1),
                node(Node::Control {
                    control: Control::If {
                        condition: Expr::Literal(Bool(false)),
                        then_body: body(2, vec![step]),
                        else_body: None,
                    },
                }),
            ]);
            unchanged(&mut game(seat, 2), &program, seat);
        }
    }
}

#[test]
fn prototype_decisions_preserve_numeric_layers_and_reject_invalid_transport() {
    let mut game = game(1, 1);
    game.execute_internal(
        &checked(vec![buff(1, 1, 1)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    let before = format!("{game:?}");
    game.decide(&Value::Null, "blocked").unwrap_err();
    assert_eq!(before, format!("{game:?}"));
    game.decide(&json!({"do":"end-phase"}), "allowed-with-layers")
        .unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["f"]["power"],
        3
    );
    // The layer leaves with its generation, so no live layer remains.
    let mut moved = super::game(1, 1);
    moved
        .execute_internal(
            &checked(vec![
                buff(1, 1, 1),
                movement::put(2, zone(ZoneKind::Field, 131), OwnerZone::Cemetery),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap();
    moved.decide(&json!({"do":"end"}), "allowed").unwrap();
    super::game(1, 1)
        .decide(&json!({"do":"end"}), "fresh")
        .unwrap();
}

#[cfg(feature = "runner")]
#[test]
fn printed_cp02_007_and_bp01_011_add_fragments_use_snapshot_stats() {
    use std::{env::var_os, fs::read_to_string, path::PathBuf};
    let snapshot = read_to_string(PathBuf::from(
        var_os("SVE_TEST_SNAPSHOT").expect("generate public card input first"),
    ))
    .unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let fragments = json!({"format":1,"kind":"effect_set","cards":{
        "CP02-007":{"status":"complete","review":"synthetic fragment harness","abilities":[]},
        "BP01-011":{"status":"complete","review":"synthetic fragment harness","abilities":[]}
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
        let actor = if seat == 1 { "P1" } else { "P2" };
        let opponent = if seat == 1 { "P2" } else { "P1" };
        for card in ["CP02-007", "BP01-011"] {
            let mut position = setup(seat, json!([]));
            position["players"][actor]["zones"]["hand"] = json!([]);
            position["players"][opponent]["zones"]["field"] = json!([]);
            position["players"][actor]["zones"]["field"] = json!([
                {"id":"s","card":card}, {"id":"f","card":"BP01-011"}
            ]);
            // Only the already-selected field follower's +1/+1 is exercised, without costs, fanfare or EX.
            let mut game = Game::new(
                Arc::clone(&catalog),
                &position,
                &Value::Null,
                &Value::Null,
                "printed-add",
            )
            .unwrap();
            let progress = game
                .execute_internal(
                    &checked(vec![
                        movement::put(
                            1,
                            Selector::Objects {
                                objects: vec![ObjectOperand::SourceHolder {}],
                            },
                            OwnerZone::Cemetery,
                        ),
                        buff(2, 1, 1),
                    ]),
                    &source(card),
                    context(seat),
                )
                .unwrap();
            let facts = changes(&progress);
            assert_eq!(facts.len(), 1);
            assert_eq!(facts[0].event.facets.first.before, Int(2));
            assert_eq!(facts[0].event.facets.first.after, Int(3));
            assert_eq!(facts[0].event.facets.rest[0].before, Int(3));
            assert_eq!(facts[0].event.facets.rest[0].after, Int(4));
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
                4
            );
        }
    }
}

fn opponents_field(scope: u64) -> Selector {
    Selector::Zone {
        players: players(PlayerRelation::Opponents),
        zone: ZoneKind::Field,
        view: SelectionView::Current {},
        binding: Binding::new(9, ScopeId(scope)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn source_attack(view: CharacteristicView) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::InformationOrZero {
        information: NumberRead::Stats {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            property: Stat::Attack,
            view,
        },
    }))
}

fn follower_source(seat: u32) -> Game {
    let mut position = setup(seat, json!([]));
    let actor = if seat == 1 { "P1" } else { "P2" };
    position["players"][actor]["zones"]["hand"] = json!([]);
    position["players"][actor]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    Game::new(catalog(), &position, &Value::Null, &Value::Null, "source").unwrap()
}

fn run(game: &mut Game, steps: Vec<Located<Node>>, seat: u32) -> Progress {
    game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
        .unwrap()
}

const fn seats(seat: u32) -> (&'static str, &'static str, View) {
    if seat == 1 {
        ("P1", "P2", View::P1)
    } else {
        ("P2", "P1", View::P2)
    }
}

#[test]
fn duplicate_subjects_in_one_instruction_receive_one_addition_and_one_fact() {
    for seat in [1, 2] {
        let mut game = game(seat, 0);
        let target = CaptureRef {
            binding: Binding::new(2, ScopeId(1)),
        };
        let progress = run(
            &mut game,
            vec![
                node(Node::Capture {
                    capture: Capture::Objects {
                        at: CaptureTime::AtResolution { scope: ScopeId(1) },
                        value: zone(ZoneKind::Field, 140),
                        into: target.clone(),
                    },
                }),
                modify(
                    1,
                    Selector::Union {
                        sets: vec![
                            zone(ZoneKind::Field, 141),
                            zone(ZoneKind::Field, 142),
                            Selector::Captured { reference: target },
                        ],
                    },
                    pair(1, 1),
                    DurationEnd::NoTemporalExpiry {},
                ),
            ],
            seat,
        );
        let facts = changes(&progress);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].event.facets.first.after, Int(3));
        assert_eq!(facts[0].event.facets.rest[0].after, Int(4));
        assert!(matches!(
            &progress.receipts[&ReceiptId(1)],
            InstructionOutcome::Modified { effect: Some(_), subjects } if subjects.len() == 1
        ));
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection["objects"]["f"]["power"], 3);
        assert_eq!(projection["objects"]["f"]["hp"], 4);
    }
}

#[test]
fn failure_after_resume_restores_pending_numeric_layers_and_completion_keeps_them() {
    let scaled = Expr::Compute(Box::new(IntOperation::Arithmetic {
        operation: Arithmetic::Multiply,
        arguments: vec![signed_binding(1), Expr::Literal(Int(i64::MAX))],
    }));
    for seat in [1, 2] {
        let (_, _, view) = seats(seat);
        let mut game = game(seat, 1);
        let pending = run(
            &mut game,
            vec![
                modify(
                    1,
                    zone(ZoneKind::Field, 150),
                    pair(1, 1),
                    DurationEnd::NoTemporalExpiry {},
                ),
                buff(2, 1, 1),
                integer_choice(3, 1),
                draw(4, 1),
                modify(
                    5,
                    zone(ZoneKind::Field, 151),
                    patch(Stat::Attack, scaled.clone()),
                    DurationEnd::NoTemporalExpiry {},
                ),
            ],
            seat,
        );
        assert!(!pending.complete);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["f"]["power"],
            4
        );
        serde_json::to_value(&game).unwrap_err();
        let question = game.internal_decision(view).unwrap();
        let before = exact_authority(&game);
        // A nonzero answer overflows only after the resumed draw has committed.
        game.answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(1)))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        let progress = game
            .answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(0)))
            .unwrap();
        assert!(progress.complete);
        assert_eq!(changes(&progress).len(), 2);
        let effects = [1, 2, 5].map(|receipt| {
            let InstructionOutcome::Modified {
                effect: Some(effect),
                ..
            } = &progress.receipts[&ReceiptId(receipt)]
            else {
                panic!("modified receipt {receipt}");
            };
            *effect
        });
        assert!(effects[0] != effects[1] && effects[1] != effects[2] && effects[0] != effects[2]);
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection["objects"]["f"]["power"], 4);
        assert_eq!(projection["objects"]["f"]["hp"], 5);
        assert_eq!(projection["objects"]["f"]["max_hp"], 5);
    }
}

#[test]
fn observations_mark_live_numeric_layers_and_belief_worlds_refuse_them() {
    for seat in [1, 2] {
        let (actor, opponent, view) = seats(seat);
        let mut game = game(seat, 1);
        let packet = game.projection(view).unwrap();
        assert!(
            packet["semantic_state"]
                .get("typed_numeric_layers")
                .is_none()
        );
        Game::from_observation(catalog(), &packet, actor, "numeric-belief").unwrap();
        run(&mut game, vec![buff(2, 1, 1)], seat);
        for view in [View::P1, View::P2, View::Referee] {
            let packet = game.projection(view).unwrap();
            assert_eq!(packet["semantic_state"]["typed_numeric_layers"], true);
            let text = packet.to_string();
            assert!(!text.contains("ir_numeric") && !text.contains("end_resolution"));
            for observer in [actor, opponent] {
                Game::from_observation(catalog(), &packet, observer, "numeric-belief").unwrap_err();
            }
        }
    }
}

#[test]
fn zero_life_waits_for_the_resolution_end_rule_check_and_added_life_absorbs_damage() {
    for seat in [1, 2] {
        let (_, opponent, _) = seats(seat);
        let mut negative = game(seat, 0);
        let progress = run(&mut negative, vec![buff(1, 0, -3), buff(2, 1, 0)], seat);
        let facts = changes(&progress);
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].event.facets.first.property, Stat::Life);
        assert_eq!(facts[0].event.facets.first.after, Int(0));
        // Zero life is not cleaned up between instructions, so the follower remains a subject.
        assert_eq!(facts[1].event.facets.first.property, Stat::Attack);
        assert_eq!(facts[1].event.facets.first.after, Int(3));
        let projection = negative.projection(View::Referee).unwrap();
        assert_eq!(projection[opponent]["field"].as_array().unwrap().len(), 0);
        assert_eq!(projection["objects"]["f"]["generation"], 1);
        let mut board = game(seat, 0);
        run(
            &mut board,
            vec![
                modify(
                    1,
                    zone(ZoneKind::Field, 170),
                    pair(0, 2),
                    DurationEnd::NoTemporalExpiry {},
                ),
                damage(
                    2,
                    RecipientSet::Objects {
                        objects: zone(ZoneKind::Field, 171),
                    },
                    3,
                ),
            ],
            seat,
        );
        let projection = board.projection(View::Referee).unwrap();
        assert_eq!(projection[opponent]["field"].as_array().unwrap().len(), 1);
        assert_eq!(projection["objects"]["f"]["hp"], 2);
        assert_eq!(projection["objects"]["f"]["damage"], 3);
    }
}

#[test]
fn effective_source_reads_follow_earlier_instructions_and_reject_stale_or_other_views() {
    for seat in [1, 2] {
        let mut game = follower_source(seat);
        run(
            &mut game,
            vec![
                modify(
                    1,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    patch(Stat::Attack, Expr::Literal(Int(1))),
                    DurationEnd::NoTemporalExpiry {},
                ),
                modify(
                    2,
                    opponents_field(180),
                    patch(Stat::Attack, source_attack(CharacteristicView::Effective)),
                    DurationEnd::NoTemporalExpiry {},
                ),
            ],
            seat,
        );
        let projection = game.projection(View::Referee).unwrap();
        assert_eq!(projection["objects"]["s"]["power"], 3);
        assert_eq!(projection["objects"]["f"]["power"], 5);
        unchanged(
            &mut follower_source(seat),
            &checked(vec![
                movement::put(
                    1,
                    Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    OwnerZone::Cemetery,
                ),
                modify(
                    2,
                    opponents_field(181),
                    patch(Stat::Attack, source_attack(CharacteristicView::Effective)),
                    DurationEnd::NoTemporalExpiry {},
                ),
            ]),
            seat,
        );
        let cost = Expr::Compute(Box::new(IntOperation::InformationOrZero {
            information: NumberRead::Cost {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                view: CharacteristicView::Effective,
            },
        }));
        for read in [source_attack(CharacteristicView::Original), cost] {
            let program = checked(vec![node(Node::Control {
                control: Control::If {
                    condition: Expr::Literal(Bool(false)),
                    then_body: body(
                        2,
                        vec![modify(
                            2,
                            opponents_field(182),
                            patch(Stat::Attack, read),
                            DurationEnd::NoTemporalExpiry {},
                        )],
                    ),
                    else_body: None,
                },
            })]);
            unchanged(&mut follower_source(seat), &program, seat);
        }
    }
}

#[test]
fn set_kind_turn_edges_and_continuous_installs_stay_rejected_beside_an_add_control() {
    let affected = Binding::new(1, ScopeId(191));
    let lifetime = Duration {
        end: DurationEnd::NoTemporalExpiry {},
        anchor: TimeAnchor::Installation {},
        usage: None,
        intervals: vec![],
    };
    let continuous = instruction(
        2,
        Action::InstallEffect {
            subjects: InstallationSubjects::Objects {
                objects: zone(ZoneKind::Field, 190),
                affected,
            },
            capture: InstallationTime::InstructionStart,
            effect: Box::new(Effect::Continuous {
                id: EffectDefinitionId(1),
                source: AbilitySource {
                    holder: ObjectOperand::SourceHolder {},
                    origin: AbilityOrigin::Printed {
                        face: FaceId("0".into()),
                        line: Nat(1),
                    },
                    controller: PlayerRef::Controller {},
                },
                target: EffectTarget::Dynamic {
                    selector: Selector::Bound {
                        reference: affected,
                    },
                },
                condition: None,
                patch: pair(1, 1),
                layer: ContinuousLayer::Numeric,
                timestamp: TimestampBasis::ProperPlay {
                    execution: ExecutionRef::Current {},
                },
                active_zones: vec![],
                duration: lifetime.clone(),
            }),
            duration: lifetime,
        },
    );
    let rejected = [
        modify(
            2,
            zone(ZoneKind::Field, 192),
            CharacteristicPatch::SetKind {
                kind: CardKind::Amulet,
            },
            DurationEnd::NoTemporalExpiry {},
        ),
        modify(
            2,
            zone(ZoneKind::Field, 193),
            pair(1, 1),
            DurationEnd::NextTurnEdge {
                player: PlayerRef::Controller {},
                edge: Edge::End,
            },
        ),
        continuous,
    ];
    for seat in [1, 2] {
        let dead = |step| {
            checked(vec![
                draw(1, 1),
                node(Node::Control {
                    control: Control::If {
                        condition: Expr::Literal(Bool(false)),
                        then_body: body(2, vec![step]),
                        else_body: None,
                    },
                }),
            ])
        };
        for step in rejected.clone() {
            unchanged(&mut game(seat, 2), &dead(step), seat);
        }
        let mut control = game(seat, 2);
        control
            .execute_internal(&dead(buff(2, 1, 1)), &source("synthetic"), context(seat))
            .unwrap();
    }
}

#[test]
fn numeric_and_permission_end_resolution_share_the_closed_body_boundary() {
    for seat in [1, 2] {
        let program = |end: DurationEnd, dead: bool| {
            let numeric = modify(2, zone(ZoneKind::Field, 201), pair(1, 1), end);
            let numeric = if dead {
                node(Node::Control {
                    control: Control::If {
                        condition: Expr::Literal(Bool(false)),
                        then_body: body(2, vec![numeric]),
                        else_body: None,
                    },
                })
            } else {
                numeric
            };
            checked(vec![
                movement::protection(1, zone(ZoneKind::Field, 200), movement::effect_cause()),
                numeric,
            ])
        };
        for dead in [false, true] {
            let mut transient = game(seat, 0);
            let progress = transient
                .execute_internal(
                    &program(DurationEnd::EndResolution {}, dead),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap();
            assert!(progress.complete);
            assert_eq!(changes(&progress).len(), usize::from(!dead));
            assert_eq!(
                transient.projection(View::Referee).unwrap()["objects"]["f"]["power"],
                2
            );
            let mut control = game(seat, 0);
            control
                .execute_internal(
                    &program(DurationEnd::NoTemporalExpiry {}, dead),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap();
            assert_eq!(
                control.projection(View::Referee).unwrap()["objects"]["f"]["power"],
                if dead { 2 } else { 3 }
            );
        }
    }
}

#[path = "numeric_set.rs"]
mod set;

#[path = "numeric_resolution.rs"]
mod resolution;
