//! Synthetic allocation programs; CP03-SL07's `NameContains` filter remains unadmitted.

use super::*;

#[expect(
    clippy::panic,
    reason = "Literal fixture shape failures must stop the test."
)]
fn partition(first: Cardinality, second: Cardinality) -> Located<Node> {
    let mut step = search(conditional(), cardinality(0, 2), hand());
    if let Node::Instruction { instruction } = &mut step.node
        && let Action::Search { destination, .. } = &mut instruction.action
    {
        *destination = Some(SearchDestination::Partition {
            destinations: vec![
                SearchPartition {
                    decision: DecisionId(11),
                    zone: hand(),
                    cardinality: first,
                },
                SearchPartition {
                    decision: DecisionId(12),
                    zone: MoveDestination::Owners {
                        zone: OwnerZone::Cemetery,
                    },
                    cardinality: second,
                },
            ],
        });
    } else {
        panic!("literal search fixture");
    }
    step
}
#[expect(clippy::panic, reason = "A missing typed domain is a test failure.")]
fn question(game: &Game, seat: u32) -> (DecisionToken, Vec<u64>, Cardinality) {
    let question = game.internal_decision(view(seat)).unwrap();
    let InputDomain::SearchPartition(domain) = question.domain else {
        panic!("partition domain");
    };
    (
        question.token,
        domain.cards.iter().map(|card| card.handle).collect(),
        domain.cardinality,
    )
}
#[expect(clippy::panic, reason = "A missing typed domain is a test failure.")]
fn search_handles(game: &Game, seat: u32) -> Vec<u64> {
    let InputDomain::Search(domain) = game.internal_decision(view(seat)).unwrap().domain else {
        panic!("search domain");
    };
    domain.cards.iter().map(|card| card.handle).collect()
}
fn rejected(game: &mut Game, seat: u32, token: DecisionToken, answer: Answer) {
    let before = exact_authority(game);
    game.answer_internal(PlayerId { seat }, token, answer)
        .unwrap_err();
    assert_eq!(
        exact_authority(game),
        before,
        "a rejected answer keeps the pending authority"
    );
}
fn stat_changes(progress: &Progress) -> usize {
    progress
        .facts
        .iter()
        .filter(|fact| matches!(fact, Fact::StatChange(_)))
        .count()
}
fn allocate(game: &mut Game, seat: u32, handles: Vec<u64>) -> Progress {
    let token = game.internal_decision(view(seat)).unwrap().token;
    game.answer_internal(PlayerId { seat }, token, Answer::SearchPartition(handles))
        .unwrap()
}

#[test]
#[expect(
    clippy::cognitive_complexity,
    reason = "The end-to-end trace compares all zero, singleton and two-stage outcomes for both seats."
)]
fn zero_one_and_two_found_follow_printed_order_and_shuffle_only_at_the_end() {
    for seat in [1, 2] {
        for count in 0..=2 {
            let mut game = game(seat, 3);
            execute(
                &mut game,
                seat,
                partition(cardinality(1, 1), cardinality(1, 1)),
            );
            let search_token = game.internal_decision(view(seat)).unwrap().token;
            let search_handles = search_handles(&game, seat);
            let progress = answer(
                &mut game,
                seat,
                SearchAnswer::Ungrouped((0..count).collect()),
            );
            if count == 0 {
                assert!(progress.complete);
                assert_eq!(outcome(&progress).found.len(), 0);
                assert_eq!(outcome(&progress).moved.len(), 0);
                assert_eq!(shuffles(&progress), 1);
                assert!(game.internal_decision(view(seat)).is_none());
                continue;
            }
            assert!(!progress.complete && progress.receipts.is_empty());
            assert_eq!(shuffles(&progress), 0);
            let (first, handles, bounds) = question(&game, seat);
            assert_eq!(first.decision, DecisionId(11));
            assert_eq!(first.execution, search_token.execution);
            assert!(first.epoch > search_token.epoch);
            assert_eq!(bounds, cardinality(1, 1));
            assert_eq!(handles.len(), usize::try_from(count).unwrap());
            assert!(!handles.iter().any(|handle| search_handles.contains(handle)));
            // A singleton find must still go to the hand; search handles stay out of scope.
            for stale in [vec![], vec![search_handles[0]]] {
                rejected(&mut game, seat, first, Answer::SearchPartition(stale));
            }
            assert!(game.internal_decision(view(3 - seat)).is_none());
            let opponent = game.projection(view(3 - seat)).unwrap();
            assert!(opponent["awaiting"].is_null());
            assert!(!opponent.to_string().contains("d2"));
            for index in 0..count {
                let id = format!("d{index}");
                assert!(
                    opponent["known_cards"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|card| card["id"] == id)
                );
                assert!(
                    !opponent["knowledge"]["located"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(id))
                );
            }
            let visible = game.projection(view(seat)).unwrap();
            assert_eq!(visible["awaiting"]["domain"]["kind"], "search-partition");
            // Answer order has no placement semantics; choose the last offered member.
            let progress = allocate(&mut game, seat, vec![*handles.last().unwrap()]);
            let result = if count == 2 {
                assert!(!progress.complete && progress.receipts.is_empty());
                assert_eq!(shuffles(&progress), 0);
                assert_eq!(progress.facts.len(), 1);
                let (second, remaining, bounds) = question(&game, seat);
                assert_eq!(second.decision, DecisionId(12));
                assert_eq!(second.execution, first.execution);
                assert!(second.epoch > first.epoch);
                assert_eq!(remaining.len(), 1);
                assert_eq!(bounds, cardinality(1, 1));
                assert!(remaining.iter().all(|handle| !handles.contains(handle)));
                let referee = game.projection(View::Referee).unwrap();
                assert!(
                    referee[seat_name(seat)]["hand"]
                        .as_array()
                        .unwrap()
                        .contains(&json!("d1"))
                );
                allocate(&mut game, seat, remaining)
            } else {
                progress
            };
            assert!(result.complete);
            assert_eq!(shuffles(&result), 1);
            let outcome = outcome(&result);
            assert_eq!(outcome.found, outcome.revealed);
            assert_eq!(outcome.found.len(), usize::try_from(count).unwrap());
            assert_eq!(outcome.moved.len(), usize::try_from(count).unwrap());
            assert_eq!(outcome.moved[0].actual_to.unwrap().kind, ZoneKind::Hand);
            if count == 2 {
                assert_eq!(outcome.moved[1].actual_to.unwrap().kind, ZoneKind::Cemetery);
                assert_eq!(outcome.moved[0].before, outcome.found[1].object);
                assert_eq!(outcome.moved[1].before, outcome.found[0].object);
            }
            for movement in &outcome.moved {
                assert_eq!(movement.before.generation, Generation(0));
                assert_eq!(movement.after.unwrap().generation, Generation(1));
            }
        }
    }
}

#[test]
fn every_stage_rejects_wrong_actor_variant_bounds_and_previous_handles_atomically() {
    for seat in [1, 2] {
        let mut game = game(seat, 3);
        execute(
            &mut game,
            seat,
            partition(cardinality(1, 1), cardinality(1, 1)),
        );
        let initial = game.internal_decision(view(seat)).unwrap().token;
        answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0, 1]));
        let (first, handles, _) = question(&game, seat);
        for stage in 0..2 {
            let (token, current, _) = question(&game, seat);
            if current.len() > 1 {
                let before = exact_authority(&game);
                game.answer_internal(
                    PlayerId { seat },
                    token,
                    Answer::SearchPartition(current.clone()),
                )
                .unwrap_err();
                assert_eq!(exact_authority(&game), before);
            }
            let wrong = vec![
                (
                    PlayerId { seat: 3 - seat },
                    token,
                    Answer::SearchPartition(vec![current[0]]),
                ),
                (
                    PlayerId { seat },
                    DecisionToken {
                        epoch: token.epoch + 1,
                        ..token
                    },
                    Answer::SearchPartition(vec![current[0]]),
                ),
                (
                    PlayerId { seat },
                    initial,
                    Answer::SearchPartition(vec![current[0]]),
                ),
                (
                    PlayerId { seat },
                    token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![current[0]])),
                ),
                (PlayerId { seat }, token, Answer::Order(vec![current[0]])),
                (PlayerId { seat }, token, Answer::SearchPartition(vec![])),
                (
                    PlayerId { seat },
                    token,
                    Answer::SearchPartition(vec![current[0], current[0]]),
                ),
                (PlayerId { seat }, token, Answer::SearchPartition(vec![0])),
                (
                    PlayerId { seat },
                    token,
                    Answer::SearchPartition(vec![u64::MAX]),
                ),
            ];
            for (actor, token, answer) in wrong {
                let before = exact_authority(&game);
                game.answer_internal(actor, token, answer).unwrap_err();
                assert_eq!(exact_authority(&game), before);
            }
            if stage == 1 {
                // handles[1] still names the unallocated card, but only in the previous question.
                for (old, answer) in [
                    (first, vec![handles[0]]),
                    (token, vec![handles[0]]),
                    (token, vec![handles[1]]),
                ] {
                    let before = exact_authority(&game);
                    game.answer_internal(PlayerId { seat }, old, Answer::SearchPartition(answer))
                        .unwrap_err();
                    assert_eq!(exact_authority(&game), before);
                }
            }
            let progress = allocate(&mut game, seat, vec![current[0]]);
            assert_eq!(progress.complete, stage == 1);
            if stage == 1 {
                let before = exact_authority(&game);
                game.answer_internal(
                    PlayerId { seat },
                    token,
                    Answer::SearchPartition(vec![current[0]]),
                )
                .unwrap_err();
                assert_eq!(exact_authority(&game), before);
            }
        }
    }
}

#[test]
fn earlier_segments_do_not_reserve_for_later_minima_and_zero_bounds_still_ask() {
    for seat in [1, 2] {
        let mut game = game(seat, 3);
        execute(
            &mut game,
            seat,
            partition(cardinality(0, 0), cardinality(3, 3)),
        );
        answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0, 1]));
        let (_, handles, bounds) = question(&game, seat);
        assert_eq!(handles.len(), 2);
        assert_eq!(bounds, cardinality(0, 0));
        allocate(&mut game, seat, vec![]);
        let (_, handles, bounds) = question(&game, seat);
        assert_eq!(bounds, cardinality(2, 2));
        let progress = allocate(&mut game, seat, handles);
        assert!(
            outcome(&progress)
                .moved
                .iter()
                .all(|m| m.actual_to.unwrap().kind == ZoneKind::Cemetery)
        );

        let mut game = super::game(seat, 3);
        execute(
            &mut game,
            seat,
            partition(cardinality(3, 3), cardinality(1, 1)),
        );
        answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0, 1]));
        let (_, handles, bounds) = question(&game, seat);
        assert_eq!(bounds, cardinality(2, 2));
        let progress = allocate(&mut game, seat, handles);
        assert!(progress.complete);
        assert!(
            outcome(&progress)
                .moved
                .iter()
                .all(|m| m.actual_to.unwrap().kind == ZoneKind::Hand)
        );
    }
}

#[test]
fn unsupported_partition_destinations_are_rejected_even_in_dead_branches() {
    let cemetery = MoveDestination::Owners {
        zone: OwnerZone::Cemetery,
    };
    for (admitted, destination) in [
        (true, vec![hand(), cemetery.clone()]),
        (false, vec![hand()]),
        (false, vec![cemetery.clone(), hand()]),
        (
            false,
            vec![
                hand(),
                MoveDestination::Owners {
                    zone: OwnerZone::Banish,
                },
            ],
        ),
        (false, vec![hand(), cemetery, hand()]),
    ] {
        let mut step = partition(cardinality(1, 1), cardinality(1, 1));
        if let Node::Instruction { instruction } = &mut step.node
            && let Action::Search {
                destination: Some(SearchDestination::Partition { destinations }),
                cardinality: bounds,
                ..
            } = &mut instruction.action
        {
            *bounds = cardinality(0, u64::try_from(destination.len()).unwrap());
            *destinations = destination
                .into_iter()
                .enumerate()
                .map(|(index, zone)| SearchPartition {
                    decision: DecisionId(u64::try_from(index).unwrap() + 20),
                    zone,
                    cardinality: cardinality(1, 1),
                })
                .collect();
        }
        let dead = node(Node::Control {
            control: Control::If {
                condition: Expr::Literal(Bool(false)),
                then_body: body(8, vec![step]),
                else_body: None,
            },
        });
        let mut game = game(1, 3);
        let before = exact_authority(&game);
        let result = game.execute_internal(
            &checked(vec![super::super::draw(9, 1), dead]),
            &source("synthetic"),
            context(1),
        );
        // The admitted Hand→Cemetery control proves the rejection comes from the destinations.
        if admitted {
            assert!(result.unwrap().complete);
            assert_ne!(exact_authority(&game), before);
        } else {
            result.unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
    }
}

#[test]
fn failure_after_final_partition_restores_pending_allocation_and_real_rng() {
    for seat in [1, 2] {
        let draws = Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation: Arithmetic::Multiply,
            arguments: vec![
                Expr::Compute(Box::new(IntOperation::Arithmetic {
                    operation: Arithmetic::Subtract,
                    arguments: vec![
                        Expr::Literal(Int(1)),
                        Expr::Compute(Box::new(IntOperation::FromNat {
                            value: Expr::Live(Box::new(NatRead::ZoneCount {
                                selector: super::super::zone(ZoneKind::Cemetery, 99),
                            })),
                        })),
                    ],
                })),
                Expr::Literal(Int(10_001)),
            ],
        }));
        let program = checked(vec![
            partition(cardinality(1, 2), cardinality(1, 1)),
            super::super::draw_value(9, draws),
        ]);
        let mut games = [game(seat, 10), game(seat, 10)];
        for game in &mut games {
            game.execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
            answer(game, seat, SearchAnswer::Ungrouped(vec![0, 1]));
        }
        let (token, handles, _) = question(&games[0], seat);
        let before = exact_authority(&games[0]);
        // Both cards to Hand leaves an empty final segment, then a real shuffle before the failing draw.
        games[0]
            .answer_internal(PlayerId { seat }, token, Answer::SearchPartition(handles))
            .unwrap_err();
        assert_eq!(exact_authority(&games[0]), before);
        // Retry with one in each zone and compare the successful shuffle with the same-seed control.
        let results = games.each_mut().map(|game| {
            let (_, handles, _) = question(game, seat);
            allocate(game, seat, vec![handles[0]]);
            let (_, remaining, _) = question(game, seat);
            allocate(game, seat, remaining)
        });
        assert!(results[0].complete);
        assert_eq!(results[0], results[1]);
        assert_eq!(exact_authority(&games[0]), exact_authority(&games[1]));
    }
}

#[test]
fn count_only_partition_keeps_search_minimum_and_does_not_reveal_hidden_faces() {
    for seat in [1, 2] {
        for count in 0..=2 {
            let mut game = game(seat, count);
            let mut step = partition(cardinality(1, 1), cardinality(1, 1));
            if let Node::Instruction { instruction } = &mut step.node
                && let Action::Search {
                    filter,
                    cardinality: bounds,
                    ..
                } = &mut instruction.action
            {
                *filter = Expr::Literal(Bool(true));
                *bounds = cardinality(2, 2);
            }
            execute(&mut game, seat, step);
            if count > 0 {
                // Allocation does not relax the count-only minimum to the conditional zero.
                let token = game.internal_decision(view(seat)).unwrap().token;
                let short = (1..u64::try_from(count).unwrap()).collect();
                rejected(
                    &mut game,
                    seat,
                    token,
                    Answer::Search(SearchAnswer::Ungrouped(short)),
                );
            }
            let mut progress = answer(
                &mut game,
                seat,
                SearchAnswer::Ungrouped((0..u64::try_from(count).unwrap()).collect()),
            );
            if !progress.complete {
                let opponent = game.projection(view(3 - seat)).unwrap();
                for index in 0..count {
                    assert!(!opponent.to_string().contains(&format!("d{index}")));
                }
            }
            while !progress.complete {
                let (_, handles, _) = question(&game, seat);
                progress = allocate(&mut game, seat, vec![handles[0]]);
            }
            assert_eq!(outcome(&progress).revealed.len(), 0);
            assert_eq!(outcome(&progress).found.len(), count);
            assert_eq!(outcome(&progress).moved.len(), count);
            assert_eq!(shuffles(&progress), 1);
        }
    }
}

#[test]
fn selection_and_allocation_vectors_do_not_define_destination_order() {
    for seat in [1, 2] {
        let mut games = [game(seat, 3), game(seat, 3)];
        let mut results = Vec::new();
        for (index, game) in games.iter_mut().enumerate() {
            execute(game, seat, partition(cardinality(2, 2), cardinality(1, 1)));
            let found = if index == 0 { vec![0, 1] } else { vec![1, 0] };
            answer(game, seat, SearchAnswer::Ungrouped(found));
            let (_, mut handles, _) = question(game, seat);
            if index == 1 {
                handles.reverse();
            }
            results.push(allocate(game, seat, handles));
        }
        assert!(results[0].complete);
        assert_eq!(results[0], results[1]);
        assert_eq!(exact_authority(&games[0]), exact_authority(&games[1]));
    }
}

#[test]
fn possibly_unallocated_found_is_rejected_by_shape_even_when_the_branch_is_dead() {
    for (admitted, first, second) in [
        (true, cardinality(1, 2), cardinality(1, 1)),
        (false, cardinality(0, 1), cardinality(0, 1)),
        (false, cardinality(1, 1), cardinality(0, 1)),
        (
            false,
            Cardinality {
                minimum: Nat(1),
                maximum: None,
            },
            cardinality(1, 1),
        ),
    ] {
        let mut step = partition(first, second);
        if first.maximum.is_none()
            && let Node::Instruction { instruction } = &mut step.node
            && let Action::Search { cardinality, .. } = &mut instruction.action
        {
            cardinality.maximum = None;
        }
        let dead = node(Node::Control {
            control: Control::If {
                condition: Expr::Literal(Bool(false)),
                then_body: body(8, vec![step]),
                else_body: None,
            },
        });
        for seat in [1, 2] {
            let mut game = game(seat, 3);
            let before = exact_authority(&game);
            let result = game.execute_internal(
                &checked(vec![super::super::draw(9, 1), dead.clone()]),
                &source("synthetic"),
                context(seat),
            );
            if admitted {
                assert!(result.unwrap().complete);
                assert_ne!(exact_authority(&game), before);
            } else {
                assert!(matches!(result.unwrap_err(), EngineFailure::Unsupported(_)));
                assert_eq!(exact_authority(&game), before);
            }
        }
    }
}

#[test]
fn numeric_set_survives_both_partition_waits_before_an_effective_addition() {
    for seat in [1, 2] {
        let mut position = setup(
            seat,
            json!([{"id":"d0","card":"unit-follower"},{"id":"d1","card":"unit-follower"}]),
        );
        position["players"][seat_name(seat)]["zones"]["hand"] = json!([]);
        position["players"][seat_name(seat)]["zones"]["field"] =
            json!([{"id":"s","card":"unit-follower"}]);
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "set-partition",
        )
        .unwrap();
        let modify = |id, change| {
            instruction(
                id,
                Action::Modify {
                    subjects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    patch: CharacteristicPatch::Numeric {
                        stat: Stat::Attack,
                        change,
                    },
                    expires: Duration {
                        end: DurationEnd::NoTemporalExpiry {},
                        anchor: TimeAnchor::Installation {},
                        usage: None,
                        intervals: vec![],
                    },
                    source: EffectReference::CurrentInstallation {},
                },
            )
        };
        let effective_attack = Expr::Compute(Box::new(IntOperation::InformationOrZero {
            information: NumberRead::Stats {
                object: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                property: Stat::Attack,
                view: CharacteristicView::Effective,
            },
        }));
        // Set then Add at one timestamp is noncommutative; the Set needs its own proper play.
        let set = game
            .execute_internal(
                &checked(vec![modify(
                    9,
                    NumericChange::Set {
                        amount: Expr::Literal(Int(5)),
                    },
                )]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
        assert!(set.complete);
        assert_eq!(stat_changes(&set), 1);
        let program = checked(vec![
            partition(cardinality(1, 1), cardinality(1, 1)),
            modify(
                8,
                NumericChange::Add {
                    amount: effective_attack,
                },
            ),
        ]);
        game.execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
            5
        );
        answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0, 1]));
        for stage in 0..2 {
            let (token, handles, _) = question(&game, seat);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
                5
            );
            rejected(&mut game, seat, token, Answer::SearchPartition(vec![]));
            let progress = allocate(&mut game, seat, vec![handles[0]]);
            assert_eq!(progress.complete, stage == 1);
            if stage == 1 {
                assert_eq!(
                    game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
                    10
                );
                assert_eq!(outcome(&progress).moved.len(), 2);
                assert_eq!(shuffles(&progress), 1);
                assert_eq!(stat_changes(&progress), 1);
            }
        }
    }
}

#[test]
fn failed_final_nonempty_segment_preserves_the_prior_hand_movement() {
    for seat in [1, 2] {
        let program = checked(vec![
            partition(cardinality(1, 1), cardinality(1, 1)),
            super::super::draw(9, 10_001),
        ]);
        let mut game = game(seat, 10);
        game.execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0, 1]));
        let (_, first, _) = question(&game, seat);
        let progress = allocate(&mut game, seat, vec![first[0]]);
        assert_eq!(progress.facts.len(), 1);
        assert_eq!(shuffles(&progress), 0);
        let (token, remaining, _) = question(&game, seat);
        assert_eq!(token.decision, DecisionId(12));
        rejected(&mut game, seat, token, Answer::SearchPartition(remaining));
        let referee = game.projection(View::Referee).unwrap();
        assert!(
            referee[seat_name(seat)]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("d0"))
        );
        assert!(
            !referee[seat_name(seat)]["cemetery"]
                .as_array()
                .unwrap()
                .contains(&json!("d1"))
        );
    }
}
