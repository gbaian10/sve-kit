//! Synthetic resolution expiry, damage preservation and caller check timing.

use super::*;

fn transient(id: u64, patch: CharacteristicPatch) -> Located<Node> {
    modify(
        id,
        zone(ZoneKind::Field, 400_u64.saturating_add(id)),
        patch,
        DurationEnd::EndResolution {},
    )
}

fn hit(id: u64, amount: i64) -> Located<Node> {
    damage(
        id,
        RecipientSet::Objects {
            objects: zone(ZoneKind::Field, 420_u64.saturating_add(id)),
        },
        amount,
    )
}

fn answer(board: &mut Game, seat: u32) -> Progress {
    let question = board.internal_decision(View::Referee).unwrap();
    board
        .answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(0)))
        .unwrap()
}

fn stats(board: &Game, attack: i64, hp: i64, maximum: i64, damage: i64) {
    let before = exact_authority(board);
    for view in [View::P1, View::P2, View::Referee] {
        let projection = board.projection(view).unwrap();
        let object = &projection["objects"]["f"];
        assert_eq!(
            object["power"], attack,
            "effective attack after expiry processing"
        );
        assert_eq!(
            object["hp"], hp,
            "effective current life after expiry processing"
        );
        assert_eq!(
            object["max_hp"], maximum,
            "undamaged life after expiry processing"
        );
        assert_eq!(object["damage"], damage, "expiry preserves current damage");
        assert_eq!(
            object["generation"], 0,
            "the follower must remain in its original field generation"
        );
    }
    assert_eq!(
        exact_authority(board),
        before,
        "projecting effective stats must be read-only"
    );
}

fn one_life(seat: u32) -> Game {
    let mut position = setup(seat, json!([]));
    let opponent = if seat == 1 { "P2" } else { "P1" };
    position["players"][opponent]["zones"]["field"][0]["state"] = json!({"hp":1,"max_hp":1});
    Game::new(catalog(), &position, &Value::Null, &Value::Null, "expiry").unwrap()
}

#[test]
fn lethal_temporary_reduction_expires_before_caller_checks_but_unbounded_does_not() {
    for seat in [1, 2] {
        let mut board = one_life(seat);
        let progress = run(
            &mut board,
            vec![transient(1, pair(0, -2)), integer_choice(2, 1)],
            seat,
        );
        assert!(!progress.complete);
        stats(&board, 2, -1, -1, 0);
        let progress = answer(&mut board, seat);
        assert!(progress.complete);
        assert_eq!(changes(&progress).len(), 1);
        assert_eq!(changes(&progress)[0].event.facets.first.after, Int(-1));
        stats(&board, 2, 1, 1, 0);
        assert!(
            board.projection(View::Referee).unwrap()["semantic_state"]
                .get("typed_numeric_layers")
                .is_none()
        );
        let mut control = one_life(seat);
        run(&mut control, vec![buff(1, 0, -2)], seat);
        assert_eq!(
            control.projection(View::Referee).unwrap()["objects"]["f"]["generation"],
            1
        );
    }
}

#[test]
fn temporary_life_with_damage_is_removed_before_checks_and_pending_never_expires() {
    for seat in [1, 2] {
        let mut board = one_life(seat);
        let progress = run(
            &mut board,
            vec![transient(1, pair(0, 2)), hit(2, 2), integer_choice(3, 1)],
            seat,
        );
        assert!(!progress.complete);
        stats(&board, 2, 1, 3, 2);
        serde_json::to_value(&board).unwrap_err();
        let question = board.internal_decision(View::Referee).unwrap();
        let before = exact_authority(&board);
        for (actor, payload) in [
            (3_u32.saturating_sub(seat), Answer::Integer(Nat(0))),
            (seat, Answer::Integer(Nat(9))),
            (seat, Answer::Order(vec![])),
        ] {
            board
                .answer_internal(PlayerId { seat: actor }, question.token, payload)
                .unwrap_err();
            assert_eq!(exact_authority(&board), before);
        }
        let progress = answer(&mut board, seat);
        assert!(progress.complete);
        assert_eq!(changes(&progress).len(), 1);
        let projection = board.projection(View::Referee).unwrap();
        assert_eq!(projection["objects"]["f"]["generation"], 1);
        let before = exact_authority(&board);
        board
            .answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(0)))
            .unwrap_err();
        assert_eq!(exact_authority(&board), before);
    }
}

#[test]
fn injured_set_expiry_preserves_new_damage_and_restores_original_base() {
    for seat in [1, 2] {
        let mut board = game(seat, 0);
        let progress = run(
            &mut board,
            vec![
                hit(1, 1),
                transient(2, set::set_pair(1)),
                hit(3, 1),
                integer_choice(4, 1),
            ],
            seat,
        );
        assert_eq!(changes(&progress).len(), 1);
        stats(&board, 1, 0, 2, 2);
        let progress = answer(&mut board, seat);
        assert!(progress.complete);
        assert_eq!(changes(&progress).len(), 1);
        stats(&board, 2, 1, 3, 2);
        let progress = run(&mut board, vec![buff(1, 1, 1)], seat);
        assert_eq!(changes(&progress)[0].event.facets.first.before, Int(2));
        assert_eq!(changes(&progress)[0].event.facets.rest[0].before, Int(1));
        stats(&board, 3, 2, 4, 2);
    }
}

#[test]
fn expiring_earlier_add_recomputes_remaining_set_by_proper_play() {
    for seat in [1, 2] {
        let mut board = game(seat, 0);
        let add = board
            .prepare_internal_play(
                &checked(vec![transient(1, pair(2, 2)), integer_choice(2, 1)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        let set = board
            .prepare_internal_play(
                &checked(vec![set::fixed(set::set_pair(1))]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        board.execute_internal_play(&set).unwrap();
        let progress = board.execute_internal_play(&add).unwrap();
        assert_eq!(changes(&progress).len(), 0);
        stats(&board, 1, 1, 1, 0);
        let progress = answer(&mut board, seat);
        assert!(progress.complete);
        assert_eq!(changes(&progress).len(), 0);
        stats(&board, 1, 1, 1, 0);
        let mut restored: Game =
            serde_json::from_value(serde_json::to_value(&board).unwrap()).unwrap();
        run(&mut restored, vec![], seat);
        stats(&restored, 1, 1, 1, 0);
        let before = exact_authority(&restored);
        restored
            .decide(&json!({"do":"end-phase"}), "still-layered")
            .unwrap_err();
        assert_eq!(exact_authority(&restored), before);
    }
}

#[test]
fn expiring_set_reveals_remaining_add_and_multiple_expiries_are_one_batch() {
    for seat in [1, 2] {
        let mut board = game(seat, 0);
        run(&mut board, vec![buff(1, 2, 2)], seat);
        run(
            &mut board,
            vec![transient(1, set::set_pair(1)), integer_choice(2, 1)],
            seat,
        );
        stats(&board, 1, 1, 1, 0);
        assert_eq!(changes(&answer(&mut board, seat)).len(), 1);
        stats(&board, 4, 5, 5, 0);
        let mut board = one_life(seat);
        run(
            &mut board,
            vec![
                transient(1, pair(-3, -2)),
                transient(2, pair(4, 3)),
                integer_choice(3, 1),
            ],
            seat,
        );
        stats(&board, 3, 2, 2, 0);
        assert_eq!(changes(&answer(&mut board, seat)).len(), 2);
        stats(&board, 2, 1, 1, 0);
    }
}

#[test]
fn resume_failure_after_damage_restores_pending_expiry_and_authority() {
    for seat in [1, 2] {
        let mut board = game(seat, 1);
        run(
            &mut board,
            vec![
                transient(1, pair(1, 1)),
                integer_choice(2, 1),
                hit(3, 1),
                draw(4, 1),
                buff(5, i64::MAX, 0),
            ],
            seat,
        );
        let question = board.internal_decision(View::Referee).unwrap();
        let before = exact_authority(&board);
        board
            .answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(0)))
            .unwrap_err();
        assert_eq!(exact_authority(&board), before);
        stats(&board, 3, 4, 4, 0);
    }
}

#[test]
fn overflow_revealed_by_expiry_rolls_back_the_entire_closed_execution() {
    for seat in [1, 2] {
        let mut board = game(seat, 1);
        run(
            &mut board,
            vec![modify(
                1,
                zone(ZoneKind::Field, 440),
                set::set_patch(Stat::Attack, i64::MAX),
                DurationEnd::NoTemporalExpiry {},
            )],
            seat,
        );
        let steps = vec![transient(1, pair(-1, 0)), buff(2, 1, 0), draw(3, 1)];
        unchanged(&mut board, &checked(steps.clone()), seat);
        let mut pending = steps;
        pending.push(integer_choice(4, 1));
        run(&mut board, pending, seat);
        stats(&board, i64::MAX, 3, 3, 0);
        let before = exact_authority(&board);
        let question = board.internal_decision(View::Referee).unwrap();
        board
            .answer_internal(PlayerId { seat }, question.token, Answer::Integer(Nat(0)))
            .unwrap_err();
        assert_eq!(exact_authority(&board), before);
    }
}

#[test]
fn moved_targets_do_not_reappear_and_source_departure_does_not_skip_expiry() {
    for seat in [1, 2] {
        for move_target in [true, false] {
            // The source must be a public field follower; a hand source is a rejected movement origin.
            let mut board = follower_source(seat);
            let objects = if move_target {
                zone(ZoneKind::Field, 450)
            } else {
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                }
            };
            let progress = run(
                &mut board,
                vec![
                    transient(1, pair(1, 1)),
                    movement::put(2, objects, OwnerZone::Cemetery),
                ],
                seat,
            );
            assert!(progress.complete);
            let facts = changes(&progress);
            assert_eq!(facts.len(), 2);
            assert_ne!(facts[0].event.subject, facts[1].event.subject);
            for fact in &facts {
                assert_eq!(fact.event.facets.first.before, Int(2));
                assert_eq!(fact.event.facets.first.after, Int(3));
                assert_eq!(fact.event.facets.rest[0].before, Int(3));
                assert_eq!(fact.event.facets.rest[0].after, Int(4));
            }
            assert!(matches!(
                &progress.receipts[&ReceiptId(1)],
                InstructionOutcome::Modified { effect: Some(_), subjects } if subjects.len() == 2
            ));
            let projection = board.projection(View::Referee).unwrap();
            assert_eq!(projection["objects"]["s"]["generation"], 1);
            assert!(
                projection["semantic_state"]
                    .get("typed_numeric_layers")
                    .is_none()
            );
            if move_target {
                assert_eq!(projection["objects"]["f"]["generation"], 1);
            } else {
                stats(&board, 2, 3, 3, 0);
            }
        }
    }
}

#[test]
fn effective_reads_and_instruction_snapshots_agree_before_and_after_expiry() {
    for seat in [1, 2] {
        let mut board = follower_source(seat);
        let progress = run(
            &mut board,
            vec![
                transient(1, pair(3, 2)),
                modify(
                    2,
                    opponents_field(460),
                    patch(Stat::Attack, source_attack(CharacteristicView::Effective)),
                    DurationEnd::NoTemporalExpiry {},
                ),
                integer_choice(3, 1),
            ],
            seat,
        );
        assert_eq!(changes(&progress).len(), 3);
        assert_eq!(changes(&progress)[2].event.facets.first.before, Int(5));
        assert_eq!(changes(&progress)[2].event.facets.first.after, Int(10));
        stats(&board, 10, 5, 5, 0);
        let progress = answer(&mut board, seat);
        assert_eq!(changes(&progress).len(), 3);
        stats(&board, 7, 3, 3, 0);
        let progress = run(
            &mut board,
            vec![modify(
                1,
                opponents_field(461),
                patch(Stat::Attack, source_attack(CharacteristicView::Effective)),
                DurationEnd::NoTemporalExpiry {},
            )],
            seat,
        );
        assert_eq!(changes(&progress)[0].event.facets.first.before, Int(7));
        assert_eq!(changes(&progress)[0].event.facets.first.after, Int(9));
        stats(&board, 9, 3, 3, 0);
    }
}
