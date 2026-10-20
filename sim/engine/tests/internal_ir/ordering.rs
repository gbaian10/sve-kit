//! D1-3 Order/Look fragments; no printed card is admitted as a whole ability.

use super::{
    body, catalog, checked, context, exact_authority, game, instruction, node, players, setup,
    source,
};
use alloc::sync::Arc;
use serde_json::{Value, json};
use sve_engine::catalog::Catalog;
use sve_engine::game::internal::{
    Answer, DecisionToken, InputDomain, InstructionOutcome, Progress,
};
use sve_engine::game::{Game, View};
use sve_engine::ir::*;

fn look(id: u64, count: u64) -> Located<Node> {
    instruction(
        id,
        Action::Look {
            viewer: PlayerRef::Controller {},
            zone: ZoneRef {
                player: PlayerRef::Controller {},
                kind: ZoneKind::Deck,
            },
            count: Expr::Literal(Nat(count)),
            mode: StepwiseMode::Exact,
        },
    )
}
const fn looked(id: u64) -> Selector {
    Selector::ReceiptMembers {
        receipt: ObjectReceipt::Look {
            receipt: ReceiptReference::new(ReceiptId(id)),
        },
        projection: MemberProjection::Before,
    }
}
fn order(id: u64, cards: Selector, position: Position) -> Located<Node> {
    instruction(
        id,
        Action::Order {
            decision: DecisionId(id),
            cards: cards.clone(),
            destinations: vec![Placement {
                cards,
                zone: MoveDestination::Owners {
                    zone: OwnerZone::Deck,
                },
                position,
            }],
            chooser: PlayerRef::Controller {},
            visibility: Visibility::Only {
                players: PlayerSet {
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    ..players(PlayerRelation::SelfPlayer)
                },
            },
        },
    )
}
fn unexecuted(steps: Vec<Located<Node>>) -> Located<Node> {
    node(Node::Control {
        control: Control::If {
            condition: Expr::Literal(Bool(false)),
            then_body: body(2, steps),
            else_body: None,
        },
    })
}
const fn view(seat: u32) -> View {
    if seat == 1 { View::P1 } else { View::P2 }
}
const fn opponent(seat: u32) -> View {
    if seat == 1 { View::P2 } else { View::P1 }
}
const fn seat_name(seat: u32) -> &'static str {
    if seat == 1 { "P1" } else { "P2" }
}
fn execute(game: &mut Game, seat: u32, steps: Vec<Located<Node>>) -> Progress {
    game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
        .unwrap()
}

#[test]
fn top_and_bottom_permutations_preserve_outside_order_and_generations() {
    for seat in [1, 2] {
        for (position, expected) in [
            (Position::Top {}, json!(["d2", "d0", "d1", "d3", "d4"])),
            (Position::Bottom {}, json!(["d3", "d4", "d2", "d0", "d1"])),
        ] {
            let mut game = game(seat, 5);
            let initial = game.projection(View::Referee).unwrap();
            let progress = execute(
                &mut game,
                seat,
                vec![look(1, 3), order(2, looked(1), position)],
            );
            assert!(!progress.complete && progress.facts.is_empty());
            let Some(InstructionOutcome::Look(looked)) = progress.receipts.get(&ReceiptId(1))
            else {
                panic!("look outcome");
            };
            assert_eq!(looked.looked.len(), 3);
            let question = game.internal_decision(view(seat)).unwrap();
            let InputDomain::Order(cards) = question.domain else {
                panic!("order domain");
            };
            assert_eq!(
                cards.iter().map(|card| card.handle).collect::<Vec<_>>(),
                vec![0, 1, 2]
            );
            assert!(
                cards
                    .iter()
                    .all(|card| card.card == "unit-follower" && card.name == "unit-follower")
            );
            assert!(game.internal_decision(opponent(seat)).is_none());
            assert!(game.projection(opponent(seat)).unwrap()["awaiting"].is_null());
            serde_json::to_value(&game).unwrap_err();
            game.digest().unwrap_err();
            let result = game
                .answer_internal(
                    PlayerId { seat },
                    question.token,
                    Answer::Order(vec![2, 0, 1]),
                )
                .unwrap();
            assert!(result.complete && result.facts.is_empty());
            let Some(InstructionOutcome::Order(outcome)) = result.receipts.get(&ReceiptId(2))
            else {
                panic!("order outcome");
            };
            assert_eq!(outcome.moves.len(), 0);
            assert_eq!(
                outcome
                    .successful
                    .iter()
                    .map(|member| member.before.unwrap().object)
                    .collect::<Vec<_>>(),
                vec![
                    looked.looked[2].object,
                    looked.looked[0].object,
                    looked.looked[1].object
                ]
            );
            assert!(
                outcome
                    .successful
                    .iter()
                    .all(|member| member.before == member.after)
            );
            let packet = game.projection(View::Referee).unwrap();
            assert_eq!(packet[seat_name(seat)]["deck"], expected);
            assert_eq!(packet["objects"], initial["objects"]);
            assert!(
                game.projection(opponent(seat)).unwrap()["objects"]
                    .get("d0")
                    .is_none()
            );
        }
    }
}

#[test]
fn invalid_permutations_actor_type_and_tokens_restore_the_entire_pending_authority() {
    for seat in [1, 2] {
        let mut game = game(seat, 4);
        execute(
            &mut game,
            seat,
            vec![look(1, 3), order(2, looked(1), Position::Bottom {})],
        );
        let question = game.internal_decision(view(seat)).unwrap();
        for handles in [
            vec![],
            vec![0, 1],
            vec![0, 1, 1],
            vec![0, 1, 3],
            vec![u64::MAX, 1, 2],
            vec![0, 1, 2, 3],
        ] {
            let before = exact_authority(&game);
            game.answer_internal(PlayerId { seat }, question.token, Answer::Order(handles))
                .unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
        for (actor, token, answer) in [
            (
                PlayerId { seat: 3 - seat },
                question.token,
                Answer::Order(vec![0, 1, 2]),
            ),
            (
                PlayerId { seat },
                DecisionToken {
                    epoch: question.token.epoch.saturating_add(1),
                    ..question.token
                },
                Answer::Order(vec![0, 1, 2]),
            ),
            (
                PlayerId { seat },
                DecisionToken {
                    execution: ExecutionId(question.token.execution.0.saturating_add(1)),
                    ..question.token
                },
                Answer::Order(vec![0, 1, 2]),
            ),
            (
                PlayerId { seat },
                DecisionToken {
                    decision: DecisionId(99),
                    ..question.token
                },
                Answer::Order(vec![0, 1, 2]),
            ),
            (PlayerId { seat }, question.token, Answer::Integer(Nat(1))),
        ] {
            let before = exact_authority(&game);
            game.answer_internal(actor, token, answer).unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
        game.answer_internal(
            PlayerId { seat },
            question.token,
            Answer::Order(vec![1, 2, 0]),
        )
        .unwrap();
        let before = exact_authority(&game);
        game.answer_internal(
            PlayerId { seat },
            question.token,
            Answer::Order(vec![1, 2, 0]),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn empty_order_is_automatic_but_one_card_still_requires_a_complete_answer() {
    for seat in [1, 2] {
        for count in [0, 1] {
            let mut game = game(seat, 3);
            let progress = execute(
                &mut game,
                seat,
                vec![look(1, count), order(2, looked(1), Position::Top {})],
            );
            assert_eq!(progress.complete, count == 0);
            if count == 0 {
                assert_eq!(
                    progress.receipts.get(&ReceiptId(2)),
                    Some(&InstructionOutcome::Order(OrderOutcome {
                        successful: vec![],
                        moves: vec![]
                    }))
                );
                assert!(game.internal_decision(view(seat)).is_none());
            } else {
                let question = game.internal_decision(view(seat)).unwrap();
                let before = exact_authority(&game);
                game.answer_internal(PlayerId { seat }, question.token, Answer::Order(vec![]))
                    .unwrap_err();
                assert_eq!(exact_authority(&game), before);
                game.answer_internal(PlayerId { seat }, question.token, Answer::Order(vec![0]))
                    .unwrap();
            }
        }
    }
}

#[test]
fn subsequent_orders_revoke_old_evidence_and_use_new_epochs() {
    for seat in [1, 2] {
        let mut game = game(seat, 4);
        execute(
            &mut game,
            seat,
            vec![
                look(1, 3),
                order(2, looked(1), Position::Bottom {}),
                look(3, 2),
                order(4, looked(3), Position::Top {}),
            ],
        );
        let first = game.internal_decision(view(seat)).unwrap();
        game.answer_internal(PlayerId { seat }, first.token, Answer::Order(vec![1, 2, 0]))
            .unwrap();
        let second = game.internal_decision(view(seat)).unwrap();
        assert!(second.token.epoch > first.token.epoch);
        let before = exact_authority(&game);
        game.answer_internal(PlayerId { seat }, first.token, Answer::Order(vec![0, 1]))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        game.answer_internal(PlayerId { seat }, second.token, Answer::Order(vec![1, 0]))
            .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat_name(seat)]["deck"],
            json!(["d1", "d3", "d2", "d0"])
        );

        let mut stale = super::game(seat, 4);
        execute(
            &mut stale,
            seat,
            vec![
                look(1, 3),
                order(2, looked(1), Position::Bottom {}),
                order(3, looked(1), Position::Top {}),
            ],
        );
        let question = stale.internal_decision(view(seat)).unwrap();
        let before = exact_authority(&stale);
        stale
            .answer_internal(
                PlayerId { seat },
                question.token,
                Answer::Order(vec![0, 1, 2]),
            )
            .unwrap_err();
        assert_eq!(exact_authority(&stale), before);
    }
}

#[test]
fn different_private_orders_have_identical_opponent_packets() {
    for seat in [1, 2] {
        let mut first = game(seat, 4);
        let mut second = first.clone();
        let steps = vec![look(1, 3), order(2, looked(1), Position::Bottom {})];
        execute(&mut first, seat, steps.clone());
        execute(&mut second, seat, steps);
        let question = first.internal_decision(view(seat)).unwrap();
        let packet = first.projection(view(seat)).unwrap();
        assert_eq!(packet["awaiting"]["domain"]["cards"][0]["handle"], 0);
        assert!(packet["awaiting"]["domain"]["cards"][0].get("id").is_none());
        for game in [&first, &second] {
            let packet = game.projection(opponent(seat)).unwrap().to_string();
            for id in ["d0", "d1", "d2", "d3"] {
                assert!(!packet.contains(id));
            }
        }
        first
            .answer_internal(
                PlayerId { seat },
                question.token,
                Answer::Order(vec![2, 0, 1]),
            )
            .unwrap();
        second
            .answer_internal(
                PlayerId { seat },
                question.token,
                Answer::Order(vec![1, 2, 0]),
            )
            .unwrap();
        assert_eq!(
            first.projection(opponent(seat)).unwrap(),
            second.projection(opponent(seat)).unwrap()
        );
        assert_ne!(
            first.projection(View::Referee).unwrap(),
            second.projection(View::Referee).unwrap()
        );
    }
}

#[test]
fn look_clamps_to_the_deck_and_order_difference_uses_the_frozen_observation() {
    for seat in [1, 2] {
        let mut game = game(seat, 4);
        let cards = Selector::Difference {
            input: Box::new(looked(1)),
            except: Box::new(looked(2)),
        };
        let progress = execute(
            &mut game,
            seat,
            vec![
                look(1, 99),
                look(2, 1),
                order(3, cards, Position::Bottom {}),
            ],
        );
        let Some(InstructionOutcome::Look(outcome)) = progress.receipts.get(&ReceiptId(1)) else {
            panic!("look outcome");
        };
        assert_eq!(outcome.looked.len(), 4);
        let question = game.internal_decision(view(seat)).unwrap();
        game.answer_internal(
            PlayerId { seat },
            question.token,
            Answer::Order(vec![2, 0, 1]),
        )
        .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat_name(seat)]["deck"],
            json!(["d0", "d3", "d1", "d2"])
        );
    }
}

#[test]
fn unsupported_look_and_dead_private_reads_are_rejected_before_any_mutation() {
    let mut stepwise = look(1, 2);
    if let Node::Instruction { instruction } = &mut stepwise.node
        && let Action::Look { mode, .. } = &mut instruction.action
    {
        *mode = StepwiseMode::UpToStepwise {
            decision: Box::new(StepwiseDecision {
                id: DecisionId(5),
                actor: PlayerRef::Controller {},
                visibility: Visibility::Only {
                    players: PlayerSet {
                        cardinality: Cardinality {
                            minimum: Nat(1),
                            maximum: Some(Nat(1)),
                        },
                        ..players(PlayerRelation::SelfPlayer)
                    },
                },
                timing: StepwiseTiming::AfterLook,
            }),
        };
    }
    let mut foreign = look(1, 2);
    if let Node::Instruction { instruction } = &mut foreign.node
        && let Action::Look { zone, .. } = &mut instruction.action
    {
        zone.player = PlayerRef::RequireOne {
            players: Box::new(PlayerSet {
                cardinality: Cardinality {
                    minimum: Nat(1),
                    maximum: Some(Nat(1)),
                },
                ..players(PlayerRelation::Opponents)
            }),
        };
    }
    let mut other_zone = look(1, 2);
    if let Node::Instruction { instruction } = &mut other_zone.node
        && let Action::Look { zone, .. } = &mut instruction.action
    {
        zone.kind = ZoneKind::Hand;
    }
    let mut hidden_count = look(1, 2);
    if let Node::Instruction { instruction } = &mut hidden_count.node
        && let Action::Look { count, .. } = &mut instruction.action
    {
        *count = Expr::Live(Box::new(NatRead::ZoneCount {
            selector: super::zone(ZoneKind::Deck, 90),
        }));
    }
    let private_move = instruction(
        2,
        Action::Move {
            objects: looked(1),
            movement: MoveSpec {
                to: MoveDestination::Owners {
                    zone: OwnerZone::Cemetery,
                },
                initial_posture: None,
                entry_effects: vec![],
                position: None,
                order: None,
            },
            reason: MoveReason::Put,
        },
    );
    let dead = unexecuted(vec![node(Node::Control {
        control: Control::If {
            condition: Expr::Compute(Box::new(Predicate::Exists { objects: looked(1) })),
            then_body: body(3, vec![]),
            else_body: None,
        },
    })]);
    for steps in [
        vec![stepwise],
        vec![foreign],
        vec![other_zone],
        vec![unexecuted(vec![hidden_count])],
        vec![look(1, 2), private_move.clone()],
        // Unexecuted forms prove admission, not the runtime evaluator, rejects them.
        vec![look(1, 2), unexecuted(vec![private_move])],
        vec![look(1, 2), dead],
    ] {
        let mut game = game(1, 4);
        let before = exact_authority(&game);
        game.execute_internal(&checked(steps), &source("synthetic"), context(1))
            .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn private_reads_nested_under_order_leaf_selectors_are_rejected_even_when_dead() {
    let dead = |cards| unexecuted(vec![order(2, cards, Position::Bottom {})]);
    let private_capture = node(Node::Capture {
        capture: Capture::Objects {
            at: CaptureTime::AtResolution { scope: ScopeId(1) },
            value: looked(1),
            into: CaptureRef {
                binding: Binding::new(7, ScopeId(2)),
            },
        },
    });
    // D0 already rejects capturing a private Look; admission is not the only guard here.
    WellFormedProgram::check(body(1, vec![look(1, 2), unexecuted(vec![private_capture])]))
        .unwrap_err();
    let Selector::Zone {
        players,
        zone,
        view: selection,
        binding,
        ..
    } = super::zone(ZoneKind::Field, 91)
    else {
        panic!("zone selector");
    };
    let nested = Selector::Zone {
        players,
        zone,
        view: selection,
        binding,
        predicate: Box::new(Expr::Compute(Box::new(Predicate::Exists {
            objects: looked(1),
        }))),
    };
    for seat in [1, 2] {
        let mut game = game(seat, 4);
        let before = exact_authority(&game);
        game.execute_internal(
            &checked(vec![look(1, 2), dead(nested.clone())]),
            &source("synthetic"),
            context(seat),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        // The same dead Order over the Look receipt itself remains admissible.
        let progress = execute(&mut game, seat, vec![look(1, 2), dead(looked(1))]);
        assert!(progress.complete && game.internal_decision(view(seat)).is_none());
    }
}

#[test]
fn public_order_even_in_dead_branches_and_non_deck_domains_fail_atomically() {
    let mut public = order(2, looked(1), Position::Bottom {});
    if let Node::Instruction { instruction } = &mut public.node
        && let Action::Order { visibility, .. } = &mut instruction.action
    {
        *visibility = Visibility::Public {};
    }
    // D0 correctly rejects private Look as a public choice; runtime also requires same-deck generations.
    WellFormedProgram::check(body(1, vec![look(1, 3), public])).unwrap_err();
    for seat in [1, 2] {
        let mut game = game(seat, 3);
        let before = exact_authority(&game);
        let cards = Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        };
        game.execute_internal(
            &checked(vec![order(1, cards.clone(), Position::Top {})]),
            &source("synthetic"),
            context(seat),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        // A public question has no admitted projection contract, including when its branch is dead.
        let mut position = setup(
            seat,
            json!([{"id":"s","card":"unit-spell"},{"id":"d0","card":"unit-follower"}]),
        );
        position["players"][seat_name(seat)]["zones"]["hand"] = json!([]);
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "order-public",
        )
        .unwrap();
        let before = exact_authority(&game);
        // Any audience other than exactly the chooser has no admitted projection contract.
        for audience in [
            Visibility::Public {},
            Visibility::Only {
                players: PlayerSet {
                    cardinality: Cardinality {
                        minimum: Nat(1),
                        maximum: Some(Nat(1)),
                    },
                    ..players(PlayerRelation::Opponents)
                },
            },
        ] {
            let mut shared = order(2, cards.clone(), Position::Top {});
            if let Node::Instruction { instruction } = &mut shared.node
                && let Action::Order { visibility, .. } = &mut instruction.action
            {
                *visibility = audience;
            }
            for order in [shared.clone(), unexecuted(vec![shared])] {
                game.execute_internal(
                    &checked(vec![look(1, 1), order]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap_err();
                assert_eq!(exact_authority(&game), before);
            }
        }
    }
}

#[test]
fn foreign_owner_anonymous_members_and_changed_generations_cannot_enter_an_order() {
    for seat in [1, 2] {
        for deck in [
            json!([{"id":"d0","card":"unit-follower","owner":seat_name(3-seat)}]),
            json!([{"filler":1}]),
        ] {
            let mut game = Game::new(
                catalog(),
                &setup(seat, deck),
                &Value::Null,
                &Value::Null,
                "order-invalid-deck",
            )
            .unwrap();
            let before = exact_authority(&game);
            game.execute_internal(
                &checked(vec![look(1, 1), order(2, looked(1), Position::Bottom {})]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
        let mut position = setup(
            seat,
            json!([{"id":"s","card":"unit-spell"},{"id":"d0","card":"unit-follower"}]),
        );
        position["players"][seat_name(seat)]["zones"]["hand"] = json!([]);
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &Value::Null,
            "order-generation",
        )
        .unwrap();
        let before = exact_authority(&game);
        let source_object = Selector::Objects {
            objects: vec![ObjectOperand::SourceHolder {}],
        };
        game.execute_internal(
            &checked(vec![
                look(1, 1),
                super::draw(2, 1),
                order(3, source_object.clone(), Position::Bottom {}),
            ]),
            &source("synthetic"),
            context(seat),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        execute(
            &mut game,
            seat,
            vec![look(1, 1), order(2, source_object, Position::Bottom {})],
        );
        let question = game.internal_decision(view(seat)).unwrap();
        game.answer_internal(PlayerId { seat }, question.token, Answer::Order(vec![0]))
            .unwrap();
    }
}

#[cfg(feature = "runner")]
#[test]
fn printed_bp07_sl09_and_bp06_024_zero_selected_remainder_fragments() {
    use std::{env::var_os, fs::read_to_string, path::PathBuf};
    let snapshot = read_to_string(PathBuf::from(
        var_os("SVE_TEST_SNAPSHOT").expect("generate public card input first"),
    ))
    .unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    // Zero selected is a legal fragment; selection, summation, field entry and other abilities stay outside it.
    let fragments = json!({"format":1,"kind":"effect_set","cards":{
        "BP07-SL09":{"status":"complete","review":"synthetic fragment harness","abilities":[]},
        "BP06-024":{"status":"complete","review":"synthetic fragment harness","abilities":[]}
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
    for (card, name) in [
        ("BP07-SL09", "アイアロンの支配者・ベルフォメット"),
        ("BP06-024", "貴族の舞踏"),
    ] {
        for seat in [1, 2] {
            let mut position = setup(
                seat,
                json!([{"id":"d0","card":"BP07-SL09"},{"id":"d1","card":"BP06-024"},{"id":"d2","card":"BP07-SL09"},{"id":"d3","card":"BP06-024"},{"id":"d4","card":"BP07-SL09"},{"id":"d5","card":"BP06-024"}]),
            );
            position["players"][seat_name(seat)]["zones"]["hand"][0]["card"] = json!(card);
            position["players"][seat_name(3 - seat)]["zones"]["field"] = json!([]);
            let mut game = Game::new(
                Arc::clone(&catalog),
                &position,
                &Value::Null,
                &Value::Null,
                "printed-remainder",
            )
            .unwrap();
            assert_eq!(
                game.projection(view(seat)).unwrap()["objects"]["s"]["name"],
                name
            );
            game.execute_internal(
                &checked(vec![look(1, 5), order(2, looked(1), Position::Bottom {})]),
                &source(card),
                context(seat),
            )
            .unwrap();
            let question = game.internal_decision(view(seat)).unwrap();
            let result = game
                .answer_internal(
                    PlayerId { seat },
                    question.token,
                    Answer::Order(vec![4, 2, 0, 3, 1]),
                )
                .unwrap();
            assert!(result.complete && result.facts.is_empty());
            assert_eq!(
                game.projection(View::Referee).unwrap()[seat_name(seat)]["deck"],
                json!(["d5", "d4", "d2", "d0", "d3", "d1"])
            );
        }
    }
}
