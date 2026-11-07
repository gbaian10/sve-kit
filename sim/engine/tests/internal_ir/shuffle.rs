//! Shuffling closes temporary placement order without exposing a decision or position.
use super::ordering::{look, looked};
use super::*;

fn shuffle(id: u64, scope: ShuffleScope) -> Located<Node> {
    instruction(
        id,
        Action::Shuffle {
            deck: PlayerRef::Controller {},
            scope,
            randomness: RandomnessPolicy::FreshRequest,
        },
    )
}

fn place(id: u64, objects: Selector, position: Option<Position>) -> Located<Node> {
    instruction(
        id,
        Action::Move {
            objects,
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                },
                position,
                order: None,
                initial_posture: None,
                entry_effects: vec![],
            },
            reason: MoveReason::Put,
        },
    )
}

fn subset(id: u64, movement: u64) -> Located<Node> {
    shuffle(
        id,
        ShuffleScope::Subset {
            cards: Selector::MoveAfter {
                receipt: MovementReceipt::Move {
                    receipt: ReceiptReference::new(ReceiptId(movement)),
                },
                permission: FollowPermission::SameEffectExplicitReference {},
            },
        },
    )
}

fn grave() -> Selector {
    Selector::Zone {
        players: PlayerSet {
            relative_to: PlayerRef::Controller {},
            relation: PlayerRelation::SelfPlayer,
            cardinality: Cardinality {
                minimum: Nat(1),
                maximum: Some(Nat(1)),
            },
            text_quantifier: TextQuantifier::All,
        },
        zone: ZoneKind::Cemetery,
        view: SelectionView::Current {},
        binding: Binding::new(700, ScopeId(1)),
        predicate: Box::new(Expr::Literal(Bool(true))),
    }
}

fn fixture(seat: u32, count: usize, cemetery: usize, random: &Value) -> Game {
    let actor = if seat == 1 { "P1" } else { "P2" };
    let deck = (0..count)
        .map(|i| json!({"id":format!("d{i}"),"card":"unit-follower"}))
        .collect::<Vec<_>>();
    let mut state = setup(seat, json!(deck));
    state["players"][actor]["zones"]["cemetery"] = json!(
        (0..cemetery)
            .map(|i| json!({"id":format!("g{i}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    Game::new(catalog(), &state, &Value::Null, random, "shuffle").unwrap()
}

fn execute(game: &mut Game, seat: u32, steps: Vec<Located<Node>>) -> Progress {
    game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
        .unwrap()
}

#[test]
fn full_shuffle_is_a_fact_for_empty_singleton_and_many_cards_in_both_seats() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        for count in [0, 1, 4] {
            let result = (0..count)
                .rev()
                .map(|i| format!("d{i}"))
                .collect::<Vec<_>>();
            let mut game = fixture(
                seat,
                count,
                0,
                &json!({"shuffles":[{"player":actor,"result":result}]}),
            );
            let before = game.projection(View::Referee).unwrap();
            let progress = execute(&mut game, seat, vec![shuffle(1, ShuffleScope::FullZone {})]);
            assert!(progress.complete);
            assert_eq!(
                progress.facts,
                vec![Fact::Shuffled {
                    player: PlayerId { seat }
                }]
            );
            assert_eq!(
                progress.receipts.get(&ReceiptId(1)),
                Some(&InstructionOutcome::Shuffled)
            );
            let after = game.projection(View::Referee).unwrap();
            assert_eq!(after[actor]["deck"], json!(result));
            for i in 0..count {
                assert_eq!(
                    after["objects"][format!("d{i}")]["generation"],
                    before["objects"][format!("d{i}")]["generation"]
                );
            }
        }
    }
}

#[test]
fn cemetery_batch_uses_one_move_then_one_full_shuffle_even_with_zero_returns() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        for count in [0, 3] {
            let mut result = (0..count).map(|i| format!("g{i}")).collect::<Vec<_>>();
            result.extend(["d1".into(), "d0".into()]);
            let mut game = fixture(
                seat,
                2,
                count,
                &json!({"shuffles":[{"player":actor,"result":result}]}),
            );
            let progress = execute(
                &mut game,
                seat,
                vec![
                    place(1, grave(), None),
                    shuffle(2, ShuffleScope::FullZone {}),
                ],
            );
            let Some(InstructionOutcome::Move(outcome)) = progress.receipts.get(&ReceiptId(1))
            else {
                panic!("return result");
            };
            assert_eq!(outcome.moved.len(), count);
            assert!(outcome.moved.iter().all(|movement| {
                movement
                    .after
                    .is_some_and(|after| after.generation.0 == movement.before.generation.0 + 1)
            }));
            assert_eq!(
                progress
                    .facts
                    .iter()
                    .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
                    .count(),
                1
            );
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view[actor]["cemetery"], json!([]));
            assert_eq!(view[actor]["deck"], json!(result));
        }
    }
}

#[test]
fn bottom_subset_preserves_the_original_look_generations_and_outside_order_without_a_question() {
    for seat in [1, 2] {
        let actor = if seat == 1 { "P1" } else { "P2" };
        for count in [0, 1, 3] {
            let result = (0..count)
                .rev()
                .map(|i| format!("d{i}"))
                .collect::<Vec<_>>();
            let mut game = fixture(
                seat,
                5,
                0,
                &json!({"shuffles":[{"player":actor,"result":result}]}),
            );
            let mut control = game.clone();
            let original = execute(
                &mut control,
                seat,
                vec![look(1, u64::try_from(count).unwrap())],
            );
            let progress = execute(
                &mut game,
                seat,
                vec![
                    look(1, u64::try_from(count).unwrap()),
                    place(2, looked(1), Some(Position::Bottom {})),
                    subset(3, 2),
                ],
            );
            assert!(progress.complete && progress.facts.is_empty());
            assert!(
                game.internal_decision(View::P1).is_none()
                    && game.internal_decision(View::P2).is_none()
            );
            assert_eq!(
                progress.receipts.get(&ReceiptId(1)),
                original.receipts.get(&ReceiptId(1)),
                "the frozen Look receipt is never rewritten"
            );
            assert!(
                !progress.receipts.contains_key(&ReceiptId(2)),
                "the temporary placement receipt is consumed"
            );
            let expected = (count..5)
                .map(|i| format!("d{i}"))
                .chain(result)
                .collect::<Vec<_>>();
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(view[actor]["deck"], json!(expected));
            let before = control.projection(View::Referee).unwrap();
            for i in 0..5 {
                assert_eq!(
                    view["objects"][format!("d{i}")]["generation"],
                    before["objects"][format!("d{i}")]["generation"]
                );
            }
        }
    }
}

#[test]
fn malformed_or_reused_pairs_are_rejected_before_execution() {
    let mut cases = vec![
        vec![place(1, grave(), None)],
        vec![subset(1, 50)],
        vec![
            place(1, grave(), None),
            draw(2, 1),
            shuffle(3, ShuffleScope::FullZone {}),
        ],
        vec![
            look(1, 3),
            place(2, looked(1), Some(Position::Bottom {})),
            shuffle(3, ShuffleScope::FullZone {}),
        ],
        vec![
            look(1, 3),
            place(2, looked(1), Some(Position::Bottom {})),
            subset(3, 1),
        ],
    ];
    cases.push(vec![
        place(1, grave(), None),
        node(Node::Control {
            control: Control::Sequence {
                body: body(2, vec![shuffle(2, ShuffleScope::FullZone {})]),
            },
        }),
    ]);
    let mut other_player = shuffle(2, ShuffleScope::FullZone {});
    if let Node::Instruction { instruction } = &mut other_player.node
        && let Action::Shuffle { deck, .. } = &mut instruction.action
    {
        *deck = PlayerRef::Active {};
    }
    cases.push(vec![place(1, grave(), None), other_player]);
    let reader = node(Node::Capture {
        capture: Capture::Objects {
            at: CaptureTime::AtResolution { scope: ScopeId(1) },
            value: Selector::MoveAfter {
                receipt: MovementReceipt::Move {
                    receipt: ReceiptReference::new(ReceiptId(2)),
                },
                permission: FollowPermission::SameEffectExplicitReference {},
            },
            into: CaptureRef {
                binding: Binding::new(999, ScopeId(1)),
            },
        },
    });
    cases.push(vec![
        look(1, 3),
        place(2, looked(1), Some(Position::Bottom {})),
        subset(3, 2),
        reader,
    ]);
    for steps in cases {
        WellFormedProgram::check(body(1, steps)).unwrap_err();
    }
}

#[test]
fn old_look_evidence_cannot_be_reused_after_either_kind_of_shuffle() {
    for whole in [false, true] {
        let mut game = fixture(1, 5, 0, &Value::Null);
        let before = game.clone();
        let mut steps = vec![look(1, 3)];
        if whole {
            steps.push(shuffle(2, ShuffleScope::FullZone {}));
        } else {
            steps.extend([place(2, looked(1), Some(Position::Bottom {})), subset(3, 2)]);
        }
        steps.push(ordering::order(4, looked(1), Position::Top {}));
        game.execute_internal(&checked(steps), &source("synthetic"), context(1))
            .unwrap_err();
        transactions::assert_restored(&game, &before);
    }
}

#[test]
fn failed_later_instruction_restores_rng_and_script_cursor_along_with_returned_cards() {
    for random in [
        Value::Null,
        json!({"shuffles":[{"player":"P1","result":["g0","g1","g2","d0","d1"]}]}),
    ] {
        let mut game = fixture(1, 2, 3, &random);
        let before = game.clone();
        let program = checked(vec![
            place(1, grave(), None),
            shuffle(2, ShuffleScope::FullZone {}),
            instruction(
                3,
                Action::Life {
                    player: PlayerRef::Controller {},
                    change: NumericChange::Add {
                        amount: Expr::Literal(Int(i64::MAX)),
                    },
                },
            ),
        ]);
        for _ in 0..2 {
            game.execute_internal(&program, &source("synthetic"), context(1))
                .unwrap_err();
            transactions::assert_restored(&game, &before);
        }
    }
}

#[test]
fn a_wrong_subset_permutation_and_foreign_owned_return_are_atomic_errors() {
    let mut game = fixture(
        1,
        5,
        0,
        &json!({"shuffles":[{"player":"P1","result":["d0","d1","outsider"]}]}),
    );
    let before = game.clone();
    let program = checked(vec![
        look(1, 3),
        place(2, looked(1), Some(Position::Bottom {})),
        subset(3, 2),
    ]);
    for _ in 0..2 {
        game.execute_internal(&program, &source("synthetic"), context(1))
            .unwrap_err();
        transactions::assert_restored(&game, &before);
    }
    let mut state = setup(1, json!([]));
    state["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"foreign","card":"unit-follower","owner":"P2"}]);
    let mut foreign = Game::new(
        catalog(),
        &state,
        &Value::Null,
        &Value::Null,
        "foreign-shuffle",
    )
    .unwrap();
    let before = foreign.clone();
    foreign
        .execute_internal(
            &checked(vec![
                place(1, grave(), None),
                shuffle(2, ShuffleScope::FullZone {}),
            ]),
            &source("synthetic"),
            context(1),
        )
        .unwrap_err();
    transactions::assert_restored(&foreign, &before);
}

#[test]
fn different_private_subset_permutations_have_identical_opponent_packets() {
    let mut views = Vec::new();
    let mut referee = Vec::new();
    for result in [json!(["d2", "d1", "d0"]), json!(["d1", "d0", "d2"])] {
        let mut game = fixture(
            1,
            5,
            0,
            &json!({"shuffles":[{"player":"P1","result":result}]}),
        );
        execute(
            &mut game,
            1,
            vec![
                look(1, 3),
                place(2, looked(1), Some(Position::Bottom {})),
                subset(3, 2),
            ],
        );
        views.push(game.projection(View::P2).unwrap());
        referee.push(game.projection(View::Referee).unwrap()["P1"]["deck"].clone());
    }
    assert_ne!(referee[0], referee[1]);
    assert_eq!(
        views[0], views[1],
        "the opponent learns no hidden permutation or trackable reference"
    );
}
