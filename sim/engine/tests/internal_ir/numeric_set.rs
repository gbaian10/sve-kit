//! Nonnegative Set and proper-play ordering through the closed authority entry.

use super::*;

pub(super) const fn set_patch(stat: Stat, value: i64) -> CharacteristicPatch {
    CharacteristicPatch::Numeric {
        stat,
        change: NumericChange::Set {
            amount: Expr::Literal(Int(value)),
        },
    }
}

pub(super) fn set_pair(value: i64) -> CharacteristicPatch {
    CharacteristicPatch::Combined {
        patches: vec![set_patch(Stat::Attack, value), set_patch(Stat::Life, value)],
    }
}

pub(super) fn fixed(patch: CharacteristicPatch) -> Located<Node> {
    modify(
        1,
        zone(ZoneKind::Field, 301),
        patch,
        DurationEnd::NoTemporalExpiry {},
    )
}

#[test]
fn injured_set_captures_damage_once_and_later_damage_does_not_heal_on_read() {
    for seat in [1, 2] {
        let mut board = game(seat, 0);
        let progress = run(
            &mut board,
            vec![
                damage(
                    1,
                    RecipientSet::Objects {
                        objects: zone(ZoneKind::Field, 302),
                    },
                    1,
                ),
                modify(
                    2,
                    zone(ZoneKind::Field, 303),
                    set_pair(1),
                    DurationEnd::NoTemporalExpiry {},
                ),
                integer_choice(3, 1),
                node(Node::Control {
                    control: Control::Sequence {
                        body: body(
                            2,
                            vec![
                                damage(
                                    4,
                                    RecipientSet::Objects {
                                        objects: zone(ZoneKind::Field, 304),
                                    },
                                    1,
                                ),
                                integer_choice(5, 2),
                            ],
                        ),
                    },
                }),
            ],
            seat,
        );
        let facts = changes(&progress);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].event.facets.first.operation, StatOperation::Set);
        assert_eq!(facts[0].event.facets.first.delta, Int(-1));
        assert_eq!(facts[0].event.facets.rest[0].operation, StatOperation::Set);
        assert_eq!(facts[0].event.facets.rest[0].delta, Int(-1));
        let first = board.projection(View::Referee).unwrap();
        assert_eq!(first["objects"]["f"]["power"], 1);
        assert_eq!(first["objects"]["f"]["hp"], 1);
        assert_eq!(first["objects"]["f"]["max_hp"], 2);
        assert_eq!(first["objects"]["f"]["damage"], 1);
        let question = board.internal_decision(View::Referee).unwrap();
        let progress = board
            .answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(0)))
            .unwrap();
        assert_eq!(changes(&progress).len(), 1);
        let before = exact_authority(&board);
        for view in [View::P1, View::P2, View::Referee] {
            let projection = board.projection(view).unwrap();
            assert_eq!(projection["objects"]["f"]["hp"], 0);
            assert_eq!(projection["objects"]["f"]["max_hp"], 2);
            assert_eq!(projection["objects"]["f"]["damage"], 2);
            assert_eq!(projection["objects"]["f"]["generation"], 0);
        }
        assert_eq!(exact_authority(&board), before);
        let question = board.internal_decision(View::Referee).unwrap();
        let progress = board
            .answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(0)))
            .unwrap();
        assert!(progress.complete);
        assert_eq!(changes(&progress).len(), 1);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["f"]["generation"],
            1
        );
    }
}

#[test]
fn proper_play_order_overrides_installation_order_for_both_stats() {
    for seat in [1, 2] {
        for set_first in [true, false] {
            for reverse_install in [true, false] {
                let mut board = game(seat, 0);
                run(
                    &mut board,
                    vec![damage(
                        1,
                        RecipientSet::Objects {
                            objects: zone(ZoneKind::Field, 310),
                        },
                        1,
                    )],
                    seat,
                );
                let set = checked(vec![fixed(set_pair(1))]);
                let add = checked(vec![buff(1, 1, 1)]);
                let (first, second) = if set_first {
                    (&set, &add)
                } else {
                    (&add, &set)
                };
                let a = board
                    .prepare_internal_play(first, &source("synthetic"), context(seat))
                    .unwrap();
                let b = board
                    .prepare_internal_play(second, &source("synthetic"), context(seat))
                    .unwrap();
                let order = if reverse_install {
                    [b, a.clone()]
                } else {
                    [a.clone(), b]
                };
                for play in order {
                    board.execute_internal_play(&play).unwrap();
                }
                let expected = if set_first { 2 } else { 1 };
                for view in [View::P1, View::P2, View::Referee] {
                    let projection = board.projection(view).unwrap();
                    assert_eq!(projection["objects"]["f"]["power"], expected);
                    assert_eq!(projection["objects"]["f"]["hp"], expected);
                    assert_eq!(projection["objects"]["f"]["max_hp"], expected + 1);
                    assert_eq!(projection["objects"]["f"]["damage"], 1);
                }
                let before = exact_authority(&board);
                board.execute_internal_play(&a).unwrap_err();
                assert_eq!(exact_authority(&board), before);
                let mut restored: Game =
                    serde_json::from_value(serde_json::to_value(&board).unwrap()).unwrap();
                run(&mut restored, vec![], seat);
                assert_eq!(
                    restored.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
                    expected
                );
            }
        }
    }
}

#[test]
fn same_timestamp_noncommutative_writes_are_rejected_including_dead_branches() {
    for seat in [1, 2] {
        for dead in [true, false] {
            let add = buff(3, 2, 0);
            let add = if dead {
                node(Node::Control {
                    control: Control::If {
                        condition: Expr::Literal(Bool(false)),
                        then_body: body(2, vec![add]),
                        else_body: None,
                    },
                })
            } else {
                add
            };
            let program = checked(vec![draw(2, 1), fixed(set_patch(Stat::Attack, 1)), add]);
            let mut board = game(seat, 2);
            unchanged(&mut board, &program, seat);
            let before = exact_authority(&board);
            board
                .prepare_internal_play(&program, &source("synthetic"), context(seat))
                .unwrap_err();
            assert_eq!(exact_authority(&board), before);
        }
        let mut board = game(seat, 0);
        run(
            &mut board,
            vec![
                fixed(set_patch(Stat::Attack, 1)),
                modify(
                    2,
                    zone(ZoneKind::Field, 311),
                    patch(Stat::Life, Expr::Literal(Int(2))),
                    DurationEnd::NoTemporalExpiry {},
                ),
            ],
            seat,
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["f"]["power"],
            1
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
            5
        );
    }
}

#[test]
fn recursive_combined_merges_commuting_writes_without_extra_events() {
    for seat in [1, 2] {
        let mut board = game(seat, 0);
        let combined = CharacteristicPatch::Combined {
            patches: vec![
                set_patch(Stat::Attack, 1),
                CharacteristicPatch::Combined {
                    patches: vec![
                        set_patch(Stat::Attack, 1),
                        patch(Stat::Life, Expr::Literal(Int(1))),
                        patch(Stat::Life, Expr::Literal(Int(1))),
                    ],
                },
            ],
        };
        let progress = run(&mut board, vec![fixed(combined)], seat);
        let facts = changes(&progress);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].event.facets.first.operation, StatOperation::Set);
        assert_eq!(facts[0].event.facets.rest[0].operation, StatOperation::Add);
        assert_eq!(facts[0].event.facets.rest[0].after, Int(5));
        let before = exact_authority(&board);
        let mixed = checked(vec![fixed(CharacteristicPatch::Combined {
            patches: vec![
                set_patch(Stat::Attack, 1),
                patch(Stat::Attack, Expr::Literal(Int(2))),
            ],
        })]);
        unchanged(&mut board, &mixed, seat);
        assert_eq!(exact_authority(&board), before);
    }
}

#[test]
fn each_subject_captures_its_own_damage_and_set_operands_cannot_overflow() {
    for seat in [1, 2] {
        let mut board = follower_source(seat);
        let progress = run(
            &mut board,
            vec![
                damage(
                    2,
                    RecipientSet::Objects {
                        objects: Selector::Objects {
                            objects: vec![ObjectOperand::SourceHolder {}],
                        },
                    },
                    1,
                ),
                fixed(set_pair(1)),
            ],
            seat,
        );
        assert_eq!(changes(&progress).len(), 2);
        let projection = board.projection(View::Referee).unwrap();
        assert_eq!(projection["objects"]["s"]["hp"], 1);
        assert_eq!(projection["objects"]["s"]["max_hp"], 2);
        assert_eq!(projection["objects"]["f"]["hp"], 1);
        assert_eq!(projection["objects"]["f"]["max_hp"], 1);
        unchanged(
            &mut board,
            &checked(vec![draw(2, 1), fixed(set_patch(Stat::Life, i64::MAX))]),
            seat,
        );
        let dynamic = CharacteristicPatch::Numeric {
            stat: Stat::Life,
            change: NumericChange::Set {
                amount: source_attack(CharacteristicView::Effective),
            },
        };
        unchanged(&mut board, &checked(vec![fixed(dynamic)]), seat);
    }
}

#[test]
fn zero_difference_sets_install_without_facts_and_source_reads_use_effective_set() {
    for seat in [1, 2] {
        let mut board = follower_source(seat);
        let subject = || Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        };
        let progress = run(
            &mut board,
            vec![modify(
                1,
                subject(),
                set_pair(1),
                DurationEnd::NoTemporalExpiry {},
            )],
            seat,
        );
        assert_eq!(changes(&progress).len(), 1);
        let progress = run(
            &mut board,
            vec![modify(
                1,
                subject(),
                set_pair(1),
                DurationEnd::NoTemporalExpiry {},
            )],
            seat,
        );
        assert_eq!(changes(&progress).len(), 0);
        let progress = run(
            &mut board,
            vec![modify(
                2,
                opponents_field(312),
                patch(Stat::Attack, source_attack(CharacteristicView::Effective)),
                DurationEnd::NoTemporalExpiry {},
            )],
            seat,
        );
        assert_eq!(changes(&progress).len(), 1);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["f"]["power"],
            3
        );
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["s"]["hp"],
            1
        );
    }
}

#[cfg(feature = "runner")]
#[test]
fn printed_bp16_049_injured_set_fragment_uses_snapshot_follower() {
    use std::{env::var_os, fs::read_to_string, path::PathBuf};
    let snapshot = read_to_string(PathBuf::from(
        var_os("SVE_TEST_SNAPSHOT").expect("generate public card input first"),
    ))
    .unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let fragments = json!({"format":1,"kind":"effect_set","cards":{
        "BP16-049":{"status":"complete","review":"synthetic Set fragment harness","abilities":[]},
        "BP01-011":{"status":"complete","review":"synthetic target harness","abilities":[]}
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
        let (actor, opponent, _) = seats(seat);
        let mut position = setup(seat, json!([]));
        position["players"][actor]["zones"]["hand"] = json!([{"id":"s","card":"BP16-049"}]);
        position["players"][opponent]["zones"]["field"] = json!([{"id":"f","card":"BP01-011"}]);
        let mut board = Game::new(
            Arc::clone(&catalog),
            &position,
            &Value::Null,
            &Value::Null,
            "printed-set",
        )
        .unwrap();
        let program = checked(vec![
            damage(
                2,
                RecipientSet::Objects {
                    objects: zone(ZoneKind::Field, 320),
                },
                1,
            ),
            fixed(set_pair(1)),
        ]);
        let progress = board
            .execute_internal(&program, &source("BP16-049"), context(seat))
            .unwrap();
        let facts = changes(&progress);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].event.facets.first.before, Int(2));
        assert_eq!(facts[0].event.facets.first.after, Int(1));
        assert_eq!(facts[0].event.facets.rest[0].before, Int(2));
        assert_eq!(facts[0].event.facets.rest[0].after, Int(1));
        let projection = board.projection(View::Referee).unwrap();
        assert_eq!(projection["objects"]["f"]["hp"], 1);
        assert_eq!(projection["objects"]["f"]["max_hp"], 2);
        assert_eq!(projection["objects"]["f"]["damage"], 1);
    }
}

#[test]
fn prepared_play_rechecks_context_and_blocks_prototype_mutations() {
    for seat in [1, 2] {
        let mut board = follower_source(seat);
        let mut bad = context(seat);
        bad.generation = Generation(1);
        let before = exact_authority(&board);
        board
            .prepare_internal_play(
                &checked(vec![fixed(set_pair(1))]),
                &source("synthetic"),
                bad,
            )
            .unwrap_err();
        assert_eq!(exact_authority(&board), before);
        let play = board
            .prepare_internal_play(
                &checked(vec![fixed(set_pair(1))]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        let before = format!("{board:?}");
        board
            .decide(&json!({"do":"end"}), "blocked-prepared")
            .unwrap_err();
        assert_eq!(format!("{board:?}"), before);
        for view in [View::P1, View::P2] {
            let packet = board.projection(view).unwrap();
            assert_eq!(packet["semantic_state"]["typed_numeric_layers"], true);
            assert!(!packet.to_string().contains("timestamp"));
        }
        run(
            &mut board,
            vec![movement::put(
                2,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Cemetery,
            )],
            seat,
        );
        let before = format!("{board:?}");
        board.execute_internal_play(&play).unwrap_err();
        assert_eq!(format!("{board:?}"), before);
    }
}

#[test]
fn foreign_proper_play_handles_never_select_a_local_execution() {
    for seat in [1, 2] {
        let program = checked(vec![fixed(set_pair(1))]);
        let mut first = game(seat, 0);
        let mut second = game(seat, 0);
        let foreign = first
            .prepare_internal_play(&program, &source("synthetic"), context(seat))
            .unwrap();
        let local = second
            .prepare_internal_play(&program, &source("synthetic"), context(seat))
            .unwrap();
        let before = exact_authority(&second);
        second.execute_internal_play(&foreign).unwrap_err();
        assert_eq!(exact_authority(&second), before);
        second.execute_internal_play(&local).unwrap();
        assert_eq!(
            second.projection(View::Referee).unwrap()["objects"]["f"]["hp"],
            1
        );
    }
}

#[test]
fn identical_attack_sets_have_no_delta_and_repeated_life_sets_stay_rejected() {
    for seat in [1, 2] {
        let mut board = game(seat, 0);
        let progress = run(
            &mut board,
            vec![
                fixed(set_patch(Stat::Attack, 2)),
                modify(
                    2,
                    zone(ZoneKind::Field, 330),
                    set_patch(Stat::Attack, 2),
                    DurationEnd::NoTemporalExpiry {},
                ),
            ],
            seat,
        );
        assert_eq!(changes(&progress).len(), 0);
        let effects = [1, 2].map(|id| {
            let InstructionOutcome::Modified {
                effect: Some(effect),
                ..
            } = &progress.receipts[&ReceiptId(id)]
            else {
                panic!("installed Set receipt {id}");
            };
            *effect
        });
        assert_ne!(effects[0], effects[1]);
        unchanged(
            &mut board,
            &checked(vec![
                fixed(set_patch(Stat::Life, 1)),
                damage(
                    2,
                    RecipientSet::Objects {
                        objects: zone(ZoneKind::Field, 331),
                    },
                    1,
                ),
                modify(
                    3,
                    zone(ZoneKind::Field, 332),
                    set_patch(Stat::Life, 1),
                    DurationEnd::NoTemporalExpiry {},
                ),
            ]),
            seat,
        );
        run(&mut board, vec![fixed(set_patch(Stat::Attack, 0))], seat);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["f"]["power"],
            0
        );
    }
}
