//! Successful movement projections and independent observed-deck remainder fragments.

use super::ordering::{look, looked, order};
use super::*;
use sve_engine::game::internal::{DecisionToken, InputDomain};

const fn view(seat: u32) -> View {
    if seat == 1 { View::P1 } else { View::P2 }
}
const fn seat_name(seat: u32) -> &'static str {
    if seat == 1 { "P1" } else { "P2" }
}
const fn moved(id: u64) -> MovementReceipt {
    MovementReceipt::Move {
        receipt: ReceiptReference::new(ReceiptId(id)),
    }
}
const fn before(receipt: MovementReceipt) -> Selector {
    Selector::ReceiptMembers {
        receipt: ObjectReceipt::Movement { receipt },
        projection: MemberProjection::Before,
    }
}
const fn after(receipt: MovementReceipt) -> Selector {
    Selector::MoveAfter {
        receipt,
        permission: FollowPermission::SameEffectExplicitReference {},
    }
}
fn count(objects: Selector) -> Expr<Int> {
    Expr::Compute(Box::new(IntOperation::FromNat {
        value: Expr::Compute(Box::new(NatOperation::Count { objects })),
    }))
}
fn put_ex(id: u64, objects: Selector) -> Located<Node> {
    instruction(
        id,
        Action::Move {
            objects,
            movement: MoveSpec {
                to: MoveDestination::Shared {
                    zone: ZoneRef {
                        player: PlayerRef::Controller {},
                        kind: ZoneKind::Ex,
                    },
                },
                initial_posture: None,
                entry_effects: vec![],
                position: None,
                order: None,
            },
            reason: MoveReason::Put,
        },
    )
}
fn position(seat: u32, full: bool) -> Value {
    let mut position = setup(
        seat,
        json!([
            {"id":"d0","card":"unit-follower"}, {"id":"d1","card":"unit-follower"},
            {"id":"d2","card":"unit-follower"}, {"id":"d3","card":"unit-spell"}
        ]),
    );
    position["players"][seat_name(seat)]["zones"]["ex"] = if full {
        json!([
            {"id":"e0","card":"unit-follower"}, {"id":"e1","card":"unit-follower"},
            {"id":"e2","card":"unit-follower"}, {"id":"e3","card":"unit-follower"},
            {"id":"e4","card":"unit-follower"}
        ])
    } else {
        json!([])
    };
    position
}
fn make_game(seat: u32, full: bool) -> Game {
    Game::new(
        catalog(),
        &position(seat, full),
        &Value::Null,
        &Value::Null,
        "successful-projection",
    )
    .unwrap()
}
fn execute(game: &mut Game, seat: u32, steps: Vec<Located<Node>>) -> Progress {
    game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
        .unwrap()
}
#[expect(
    clippy::panic,
    reason = "The instruction must produce its typed card outcome."
)]
fn drawn(progress: &Progress, id: u64) -> u64 {
    let Some(InstructionOutcome::Cards(outcome)) = progress.receipts.get(&ReceiptId(id)) else {
        panic!("draw receipt");
    };
    outcome.actual.0
}

#[test]
fn success_projections_preserve_old_and_new_generations_without_chasing_later_moves() {
    for seat in [1, 2] {
        let mut game = make_game(seat, false);
        let progress = execute(
            &mut game,
            seat,
            vec![
                movement::put(1, zone(ZoneKind::Field, 10), OwnerZone::Cemetery),
                // Before remains the old generation and cannot move its new cemetery incarnation.
                movement::put(2, before(moved(1)), OwnerZone::Banish),
                movement::put(3, after(moved(1)), OwnerZone::Banish),
                // Same explicit follow again does not chase the intervening banish generation.
                movement::put(4, after(moved(1)), OwnerZone::Cemetery),
                draw_value(5, count(before(moved(2)))),
                draw_value(6, count(after(moved(4)))),
                draw_value(7, count(before(moved(1)))),
            ],
        );
        assert_eq!(
            (
                drawn(&progress, 5),
                drawn(&progress, 6),
                drawn(&progress, 7)
            ),
            (0, 0, 1)
        );
        let facts = progress
            .facts
            .iter()
            .filter_map(|fact| match fact {
                Fact::Movement(fact) => Some(fact),
                Fact::AbilityPlayed { .. }
                | Fact::AbilityResolved { .. }
                | Fact::Shuffled { .. }
                | Fact::Draw(_)
                | Fact::Damage(_)
                | Fact::Posture(_)
                | Fact::StatChange(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].movement.after.unwrap(), facts[1].movement.before);
        assert_eq!(facts[1].movement.actual_to.unwrap().kind, ZoneKind::Banish);
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat_name(3 - seat)]["banish"],
            json!(["f"])
        );
    }
}

#[test]
fn forbidden_destruction_has_no_successful_before_or_after() {
    for seat in [1, 2] {
        let mut game = make_game(seat, false);
        let receipt = MovementReceipt::Destroy {
            receipt: ReceiptReference::new(ReceiptId(2)),
        };
        let progress = execute(
            &mut game,
            seat,
            vec![
                movement::protection(1, zone(ZoneKind::Field, 8), movement::effect_cause()),
                instruction(
                    2,
                    Action::Destroy {
                        objects: zone(ZoneKind::Field, 9),
                    },
                ),
                draw_value(3, count(before(receipt.clone()))),
                draw_value(4, count(after(receipt))),
            ],
        );
        assert_eq!((drawn(&progress, 3), drawn(&progress, 4)), (0, 0));
        assert_eq!(progress.facts, []);
    }
}

#[test]
fn observed_remainder_subtracts_only_successful_before_and_keeps_same_name_copies() {
    for seat in [1, 2] {
        for full in [false, true] {
            for top in [false, true] {
                let mut game = make_game(seat, full);
                let remainder = Selector::Difference {
                    input: Box::new(looked(1)),
                    except: Box::new(before(moved(3))),
                };
                let progress = execute(
                    &mut game,
                    seat,
                    vec![
                        look(1, 3),
                        look(2, 1),
                        put_ex(3, looked(2)),
                        order(
                            4,
                            remainder,
                            if top {
                                Position::Top {}
                            } else {
                                Position::Bottom {}
                            },
                        ),
                    ],
                );
                assert!(!progress.complete);
                let question = game.internal_decision(view(seat)).unwrap();
                let InputDomain::Order(cards) = &question.domain else {
                    panic!("order");
                };
                assert_eq!(cards.len(), if full { 3 } else { 2 });
                assert!(cards.iter().all(|card| card.name == "unit-follower"));
                let private = game.projection(view(3 - seat)).unwrap().to_string();
                for id in if full {
                    vec!["d0", "d1", "d2", "d3"]
                } else {
                    vec!["d1", "d2", "d3"]
                } {
                    assert!(!private.contains(id));
                }
                let domain = if full { vec![2, 0, 1] } else { vec![1, 0] };
                let before_bad = exact_authority(&game);
                for (actor, token, handles) in [
                    (PlayerId { seat: 3 - seat }, question.token, domain.clone()),
                    (
                        PlayerId { seat },
                        DecisionToken {
                            epoch: question.token.epoch + 1,
                            ..question.token
                        },
                        domain.clone(),
                    ),
                    (PlayerId { seat }, question.token, vec![0, 0]),
                    (PlayerId { seat }, question.token, vec![99]),
                ] {
                    game.answer_internal(actor, token, Answer::Order(handles))
                        .unwrap_err();
                    assert_eq!(exact_authority(&game), before_bad);
                }
                let result = game
                    .answer_internal(PlayerId { seat }, question.token, Answer::Order(domain))
                    .unwrap();
                assert!(result.complete);
                let expected = match (full, top) {
                    (true, false) => json!(["d3", "d2", "d0", "d1"]),
                    (true, true) => json!(["d2", "d0", "d1", "d3"]),
                    (false, false) => json!(["d3", "d2", "d1"]),
                    (false, true) => json!(["d2", "d1", "d3"]),
                };
                let projection = game.projection(View::Referee).unwrap();
                assert_eq!(projection[seat_name(seat)]["deck"], expected);
                assert_eq!(projection["objects"]["d0"]["generation"], u64::from(!full));
                assert_eq!(projection["objects"]["d1"]["generation"], 0);
                assert_eq!(
                    result
                        .facts
                        .iter()
                        .filter(|fact| matches!(fact, Fact::Movement(_)))
                        .count(),
                    usize::from(!full)
                );
                let after_commit = exact_authority(&game);
                game.answer_internal(PlayerId { seat }, question.token, Answer::Order(vec![]))
                    .unwrap_err();
                assert_eq!(exact_authority(&game), after_commit);
            }
        }
    }
}

#[test]
fn observed_move_rejects_partial_capacity_stale_evidence_and_unobserved_domains_atomically() {
    for seat in [1, 2] {
        let mut partial = position(seat, true);
        partial["players"][seat_name(seat)]["zones"]["ex"]
            .as_array_mut()
            .unwrap()
            .pop();
        for (position, steps) in [
            (partial, vec![look(1, 2), put_ex(2, looked(1))]),
            (
                position(seat, false),
                vec![look(1, 1), draw(2, 1), put_ex(3, looked(1))],
            ),
        ] {
            let mut game = Game::new(
                catalog(),
                &position,
                &Value::Null,
                &Value::Null,
                "invalid-observed-move",
            )
            .unwrap();
            let original = exact_authority(&game);
            game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
                .unwrap_err();
            assert_eq!(exact_authority(&game), original);
        }
        let public = put_ex(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
        );
        let mut foreign = put_ex(2, looked(1));
        if let Node::Instruction { instruction } = &mut foreign.node
            && let Action::Move { movement, .. } = &mut instruction.action
        {
            movement.to = MoveDestination::Shared {
                zone: ZoneRef {
                    player: PlayerRef::RequireOne {
                        players: Box::new(PlayerSet {
                            cardinality: Cardinality {
                                minimum: Nat(1),
                                maximum: Some(Nat(1)),
                            },
                            ..players(PlayerRelation::Opponents)
                        }),
                    },
                    kind: ZoneKind::Ex,
                },
            };
        }
        for action in [public, foreign] {
            for dead in [false, true] {
                let mut game = make_game(seat, false);
                let original = exact_authority(&game);
                let action = if dead {
                    node(Node::Control {
                        control: Control::If {
                            condition: Expr::Literal(Bool(false)),
                            then_body: body(8, vec![action.clone()]),
                            else_body: None,
                        },
                    })
                } else {
                    action.clone()
                };
                game.execute_internal(
                    &checked(vec![draw(8, 1), look(1, 1), action]),
                    &source("synthetic"),
                    context(seat),
                )
                .unwrap_err();
                assert_eq!(exact_authority(&game), original);
            }
        }
    }
}

#[test]
fn empty_observed_move_executes_with_partial_or_full_ex() {
    for seat in [1, 2] {
        for kept in [4, 5] {
            let mut layout = position(seat, true);
            layout["players"][seat_name(seat)]["zones"]["ex"]
                .as_array_mut()
                .unwrap()
                .truncate(kept);
            let mut game = Game::new(
                catalog(),
                &layout,
                &Value::Null,
                &Value::Null,
                "empty-observed-move",
            )
            .unwrap();
            let empty = Selector::Difference {
                input: Box::new(looked(1)),
                except: Box::new(looked(1)),
            };
            let progress = execute(&mut game, seat, vec![look(1, 2), put_ex(2, empty)]);
            assert!(progress.complete);
            assert!(matches!(
                progress.receipts.get(&ReceiptId(2)),
                Some(InstructionOutcome::Move(outcome)) if outcome.moved.is_empty()
            ));
            let projection = game.projection(View::Referee).unwrap();
            assert_eq!(
                projection[seat_name(seat)]["ex"].as_array().unwrap().len(),
                kept
            );
            assert_eq!(
                projection[seat_name(seat)]["deck"],
                json!(["d0", "d1", "d2", "d3"])
            );
        }
    }
}

#[test]
fn successful_after_is_usable_but_reordering_revokes_old_look_authority() {
    for seat in [1, 2] {
        let mut game = make_game(seat, false);
        let result = execute(
            &mut game,
            seat,
            vec![
                look(1, 1),
                put_ex(2, looked(1)),
                movement::put(3, after(moved(2)), OwnerZone::Cemetery),
                draw_value(4, count(before(moved(3)))),
            ],
        );
        assert_eq!(drawn(&result, 4), 1);
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat_name(seat)]["cemetery"],
            json!(["d0"])
        );

        let mut revoked = make_game(seat, false);
        execute(
            &mut revoked,
            seat,
            vec![
                look(1, 1),
                order(2, looked(1), Position::Bottom {}),
                put_ex(3, looked(1)),
            ],
        );
        let question = revoked.internal_decision(view(seat)).unwrap();
        let pending = exact_authority(&revoked);
        revoked
            .answer_internal(PlayerId { seat }, question.token, Answer::Order(vec![0]))
            .unwrap_err();
        assert_eq!(exact_authority(&revoked), pending);
    }
}

#[cfg(feature = "runner")]
#[test]
fn printed_bp18_059_and_sl14_remainder_fragments_use_successful_generations() {
    use std::{env::var_os, fs::read_to_string, path::PathBuf};
    let snapshot = read_to_string(PathBuf::from(
        var_os("SVE_TEST_SNAPSHOT").expect("generate public card input first"),
    ))
    .unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    // The first observed card is predetermined; selection, discount and the other abilities remain unadmitted.
    let fragments = json!({"format":1,"kind":"effect_set","cards":{
        "BP18-059":{"status":"complete","review":"synthetic fragment harness","abilities":[]},
        "BP18-SL14":{"status":"complete","review":"synthetic fragment harness","abilities":[]}
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
    for card in ["BP18-059", "BP18-SL14"] {
        for seat in [1, 2] {
            for full in [false, true] {
                let mut position = position(seat, full);
                position["players"][seat_name(seat)]["zones"]["hand"][0]["card"] = json!(card);
                position["players"][seat_name(seat)]["zones"]["deck"] = json!([
                    {"id":"d0","card":card}, {"id":"d1","card":card}, {"id":"d2","card":card},
                    {"id":"d3","card":card}, {"id":"d4","card":card}, {"id":"d5","card":card}
                ]);
                // Use only fragment catalogue entries in the remaining zones as well.
                position["players"][seat_name(3 - seat)]["zones"]["field"] = json!([]);
                if full {
                    for entry in position["players"][seat_name(seat)]["zones"]["ex"]
                        .as_array_mut()
                        .unwrap()
                    {
                        entry["card"] = json!(card);
                    }
                }
                let mut game = Game::new(
                    Arc::clone(&catalog),
                    &position,
                    &Value::Null,
                    &Value::Null,
                    "bp18-remainder-fragment",
                )
                .unwrap();
                assert_eq!(
                    game.projection(view(seat)).unwrap()["objects"]["s"]["name"],
                    "壊竜の六銘伝・エル"
                );
                let remainder = Selector::Difference {
                    input: Box::new(looked(1)),
                    except: Box::new(before(moved(3))),
                };
                game.execute_internal(
                    &checked(vec![
                        look(1, 5),
                        look(2, 1),
                        put_ex(3, looked(2)),
                        order(4, remainder, Position::Bottom {}),
                    ]),
                    &source(card),
                    context(seat),
                )
                .unwrap();
                let question = game.internal_decision(view(seat)).unwrap();
                let handles = if full {
                    vec![4, 3, 2, 1, 0]
                } else {
                    vec![3, 2, 1, 0]
                };
                let result = game
                    .answer_internal(PlayerId { seat }, question.token, Answer::Order(handles))
                    .unwrap();
                assert!(
                    result.complete
                        && !result
                            .facts
                            .iter()
                            .any(|fact| matches!(fact, Fact::Shuffled { .. }))
                );
                assert_eq!(
                    game.projection(View::Referee).unwrap()[seat_name(seat)]["deck"],
                    if full {
                        json!(["d5", "d4", "d3", "d2", "d1", "d0"])
                    } else {
                        json!(["d5", "d4", "d3", "d2", "d1"])
                    }
                );
            }
        }
    }
}
