//! Search fragments; no lowering, full printed ability or capability claim.

use super::{
    catalog, checked, context, exact_authority, game, instruction, players, setup, source,
};
use serde_json::{Value, json};
use sve_engine::EngineFailure;
use sve_engine::game::internal::{
    Answer, DecisionToken, Fact, InputDomain, InstructionOutcome, Progress, SearchAnswer,
};
use sve_engine::game::{Game, View};
use sve_engine::ir::*;

const fn view(seat: u32) -> View {
    if seat == 1 { View::P1 } else { View::P2 }
}
const fn seat_name(seat: u32) -> &'static str {
    if seat == 1 { "P1" } else { "P2" }
}
pub(super) const fn cardinality(minimum: u64, maximum: u64) -> Cardinality {
    Cardinality {
        minimum: Nat(minimum),
        maximum: Some(Nat(maximum)),
    }
}
fn name(binding: Binding<ObjectRef>, value: &str) -> Expr<Bool> {
    Expr::Compute(Box::new(Predicate::NamesIntersect {
        left: Expr::Live(Box::new(NamesRead::Names {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::Bound { binding }),
            },
        })),
        right: Expr::Literal(vec![CardName(value.into())]),
    }))
}
const fn movement(to: MoveDestination) -> MoveSpec {
    MoveSpec {
        to,
        initial_posture: None,
        entry_effects: vec![],
        position: None,
        order: None,
    }
}
pub(super) fn search(filter: Expr<Bool>, count: Cardinality, to: MoveDestination) -> Located<Node> {
    instruction(
        1,
        Action::Search {
            decision: DecisionId(10),
            player: PlayerRef::Controller {},
            deck: PlayerRef::Controller {},
            candidate: Binding::new(1, ScopeId(2)),
            filter,
            cardinality: count,
            groups: None,
            destination: Some(SearchDestination::Single {
                movement: movement(to),
            }),
            continuation: SearchContinuation {
                found: Binding::new(2, ScopeId(3)),
                revealed: Binding::new(3, ScopeId(3)),
            },
        },
    )
}
pub(super) const fn hand() -> MoveDestination {
    MoveDestination::Owners {
        zone: OwnerZone::Hand,
    }
}
const fn shared(kind: ZoneKind) -> MoveDestination {
    MoveDestination::Shared {
        zone: ZoneRef {
            player: PlayerRef::Controller {},
            kind,
        },
    }
}
pub(super) fn conditional() -> Expr<Bool> {
    name(Binding::new(1, ScopeId(2)), "unit-follower")
}
fn execute(game: &mut Game, seat: u32, step: Located<Node>) -> Progress {
    game.execute_internal(&checked(vec![step]), &source("synthetic"), context(seat))
        .unwrap()
}
fn answer(game: &mut Game, seat: u32, answer: SearchAnswer) -> Progress {
    let token = game.internal_decision(view(seat)).unwrap().token;
    game.answer_internal(PlayerId { seat }, token, Answer::Search(answer))
        .unwrap()
}
#[expect(clippy::panic, reason = "A missing typed receipt is a test failure.")]
fn outcome(progress: &Progress) -> &SearchOutcome {
    let Some(InstructionOutcome::Search(outcome)) = progress.receipts.get(&ReceiptId(1)) else {
        panic!("search outcome");
    };
    outcome
}
fn shuffles(progress: &Progress) -> usize {
    progress
        .facts
        .iter()
        .filter(|fact| matches!(fact, Fact::Shuffled { .. }))
        .count()
}

#[test]
fn search_successful_projections_do_not_substitute_found_or_revealed() {
    for seat in [1, 2] {
        for full in [false, true] {
            let mut position = setup(
                seat,
                json!([
                    {"id":"d0","card":"unit-follower"}, {"id":"d1","card":"unit-spell"},
                    {"id":"d2","card":"unit-spell"}, {"id":"d3","card":"unit-spell"}
                ]),
            );
            if full {
                position["players"][seat_name(seat)]["zones"]["ex"] = json!([
                    {"id":"e0","card":"unit-follower"}, {"id":"e1","card":"unit-follower"},
                    {"id":"e2","card":"unit-follower"}, {"id":"e3","card":"unit-follower"},
                    {"id":"e4","card":"unit-follower"}
                ]);
            }
            let mut game = Game::new(
                catalog(),
                &position,
                &Value::Null,
                &Value::Null,
                "search-projections",
            )
            .unwrap();
            let receipt = MovementReceipt::Search {
                receipt: ReceiptReference::new(ReceiptId(1)),
            };
            let before = Selector::ReceiptMembers {
                receipt: ObjectReceipt::Movement {
                    receipt: receipt.clone(),
                },
                projection: MemberProjection::Before,
            };
            let after = Selector::MoveAfter {
                receipt,
                permission: FollowPermission::SameEffectExplicitReference {},
            };
            let count = |objects| {
                Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Compute(Box::new(NatOperation::Count { objects })),
                }))
            };
            game.execute_internal(
                &checked(vec![
                    search(conditional(), cardinality(1, 1), shared(ZoneKind::Ex)),
                    super::draw_value(2, count(before)),
                    super::draw_value(3, count(after)),
                ]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
            let result = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0]));
            assert_eq!(outcome(&result).found.len(), 1);
            assert_eq!(outcome(&result).revealed.len(), 1);
            assert_eq!(shuffles(&result), 1);
            for id in [2, 3] {
                let Some(InstructionOutcome::Cards(outcome)) = result.receipts.get(&ReceiptId(id))
                else {
                    panic!("draw");
                };
                assert_eq!(outcome.actual.0, u64::from(!full));
            }
        }
    }
}

#[test]
fn failure_driven_by_successful_after_restores_the_execution_and_real_rng() {
    for seat in [1, 2] {
        let after = Selector::MoveAfter {
            receipt: MovementReceipt::Search {
                receipt: ReceiptReference::new(ReceiptId(1)),
            },
            permission: FollowPermission::SameEffectExplicitReference {},
        };
        let count = Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation: Arithmetic::Multiply,
            arguments: vec![
                Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Compute(Box::new(NatOperation::Count { objects: after })),
                })),
                Expr::Literal(Int(10_001)),
            ],
        }));
        let program = checked(vec![
            search(conditional(), cardinality(0, 1), shared(ZoneKind::Ex)),
            super::draw_value(9, count),
        ]);
        let mut games = [game(seat, 10), game(seat, 10)].map(|game| {
            serde_json::from_value::<Game>(serde_json::to_value(game).unwrap()).unwrap()
        });
        let origin = games[0].clone();
        for game in &mut games {
            game.execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
        }
        let token = games[0].internal_decision(view(seat)).unwrap().token;
        games[0]
            .answer_internal(
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![0])),
            )
            .unwrap_err();
        super::transactions::assert_restored(&games[0], &origin);
        games[1] = super::transactions::aligned_control(&games[0], &origin);
        for game in &mut games {
            game.execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
        }
        // Observe the next real shuffle; ChaCha's Debug output does not expose RNG state.
        let results = games
            .each_mut()
            .map(|game| answer(game, seat, SearchAnswer::Ungrouped(vec![])));
        assert_eq!(results[0], results[1]);
        super::transactions::assert_restored(&games[0], &games[1]);
    }
}

#[test]
fn candidate_existence_does_not_change_opponents_start_observation() {
    for seat in [1, 2] {
        let mut opponents = Vec::new();
        for card in ["unit-follower", "unit-spell"] {
            let mut game = Game::new(
                catalog(),
                &setup(seat, json!([{"id":"d0","card":card}])),
                &Value::Null,
                &Value::Null,
                "same-hidden-game",
            )
            .unwrap();
            let before = game.projection(view(3 - seat)).unwrap();
            let progress = execute(
                &mut game,
                seat,
                search(conditional(), cardinality(1, 1), hand()),
            );
            assert!(
                !progress.complete && progress.receipts.is_empty() && progress.facts.is_empty()
            );
            let question = game.internal_decision(view(seat)).unwrap();
            assert_eq!(question.token.decision, DecisionId(10));
            let InputDomain::Search(domain) = question.domain else {
                panic!("search domain");
            };
            assert_eq!(domain.cards.len(), usize::from(card == "unit-follower"));
            assert_eq!(domain.cardinality.minimum, Nat(0));
            assert!(game.internal_decision(view(3 - seat)).is_none());
            opponents.push(game.projection(view(3 - seat)).unwrap());
            assert!(opponents.last().unwrap()["awaiting"].is_null());
            assert_eq!(before["objects"], opponents.last().unwrap()["objects"]);
            let result = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![]));
            assert!(result.complete);
            assert_eq!(outcome(&result).found.len(), 0);
            assert_eq!(shuffles(&result), 1);
        }
        assert_eq!(opponents[0], opponents[1]);
    }
}

#[test]
fn conditional_found_proof_and_movement_keep_distinct_generations() {
    for seat in [1, 2] {
        let mut game = game(seat, 3);
        execute(
            &mut game,
            seat,
            search(conditional(), cardinality(1, 1), hand()),
        );
        let token = game.internal_decision(view(seat)).unwrap().token;
        let result = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![1]));
        let outcome = outcome(&result);
        assert_eq!(outcome.found, outcome.revealed);
        assert_eq!(outcome.found.len(), 1);
        let moved = &outcome.moved[0];
        assert_eq!(moved.before, outcome.found[0].object);
        assert_eq!(moved.before.generation, Generation(0));
        assert_eq!(moved.after.unwrap().generation, Generation(1));
        assert_eq!(moved.actual_to.unwrap().kind, ZoneKind::Hand);
        assert_eq!(shuffles(&result), 1);
        assert_eq!(result.facts.len(), 2);
        let packet = game.projection(view(3 - seat)).unwrap();
        assert!(packet["objects"].get("d1").is_none());
        assert!(
            !packet[seat_name(seat)]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("d1"))
        );
        for id in ["d0", "d2"] {
            assert!(!packet.to_string().contains(id));
        }
        let before = exact_authority(&game);
        game.answer_internal(
            PlayerId { seat },
            token,
            Answer::Search(SearchAnswer::Ungrouped(vec![1])),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn count_only_search_must_find_available_cards_without_automatic_proof() {
    for seat in [1, 2] {
        for count in [0, 1, 2, 3] {
            let mut game = game(seat, count);
            execute(
                &mut game,
                seat,
                search(Expr::Literal(Bool(true)), cardinality(2, 2), hand()),
            );
            let token = game.internal_decision(view(seat)).unwrap().token;
            if count > 0 {
                let before = exact_authority(&game);
                game.answer_internal(
                    PlayerId { seat },
                    token,
                    Answer::Search(SearchAnswer::Ungrouped(vec![])),
                )
                .unwrap_err();
                assert_eq!(exact_authority(&game), before);
            }
            let handles = (0..u64::try_from(count.min(2)).unwrap()).collect();
            let result = answer(&mut game, seat, SearchAnswer::Ungrouped(handles));
            assert_eq!(outcome(&result).found.len(), count.min(2));
            assert_eq!(outcome(&result).revealed.len(), 0);
            assert_eq!(shuffles(&result), 1);
        }
    }
}

#[test]
fn wrong_actor_token_variant_duplicates_and_foreign_handles_keep_pending_unchanged() {
    for seat in [1, 2] {
        let mut game = game(seat, 3);
        execute(
            &mut game,
            seat,
            search(conditional(), cardinality(1, 1), hand()),
        );
        let token = game.internal_decision(view(seat)).unwrap().token;
        for (actor, token, answer) in [
            (
                PlayerId { seat: 3 - seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![0])),
            ),
            (
                PlayerId { seat },
                DecisionToken {
                    epoch: token.epoch + 1,
                    ..token
                },
                Answer::Search(SearchAnswer::Ungrouped(vec![0])),
            ),
            (PlayerId { seat }, token, Answer::Order(vec![0])),
            (
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Grouped(vec![vec![0]])),
            ),
            (
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![0, 0])),
            ),
            (
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![0, 1])),
            ),
            (
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![3])),
            ),
            (
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![u64::MAX])),
            ),
        ] {
            let before = exact_authority(&game);
            game.answer_internal(actor, token, answer).unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
        assert!(answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0])).complete);
    }
}

#[test]
fn full_ex_keeps_found_but_never_invents_success_and_always_shuffles() {
    for seat in [1, 2] {
        for full in [false, true] {
            let mut position = setup(seat, json!([{"id":"d0","card":"unit-follower"}]));
            if full {
                position["players"][seat_name(seat)]["zones"]["ex"] = json!(
                    (0..5)
                        .map(|i| json!({"id":format!("x{i}"),"card":"unit-follower"}))
                        .collect::<Vec<_>>()
                );
            }
            let mut game = Game::new(
                catalog(),
                &position,
                &Value::Null,
                &Value::Null,
                "ex-search",
            )
            .unwrap();
            execute(
                &mut game,
                seat,
                search(conditional(), cardinality(1, 1), shared(ZoneKind::Ex)),
            );
            let result = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0]));
            let outcome = outcome(&result);
            assert_eq!(outcome.found.len(), 1);
            assert_eq!(outcome.revealed, outcome.found);
            assert_eq!(outcome.moved.len(), 1);
            assert_eq!(outcome.moved[0].after.is_none(), full);
            assert_eq!(outcome.moved[0].actual_to.is_none(), full);
            assert_eq!(shuffles(&result), 1);
            let packet = game.projection(view(3 - seat)).unwrap();
            assert_eq!(packet["objects"].get("d0").is_none(), full);
            assert_eq!(packet[seat_name(seat)]["deck_count"], u64::from(full));
        }
    }
}

#[expect(
    clippy::panic,
    reason = "Literal fixture shape failures must stop the test."
)]
fn grouped() -> Located<Node> {
    let mut step = search(
        Expr::Literal(Bool(true)),
        cardinality(0, 3),
        shared(ZoneKind::Field),
    );
    let Node::Instruction { instruction } = &mut step.node else {
        panic!("literal fixture shape");
    };
    let Action::Search { groups, .. } = &mut instruction.action else {
        panic!("literal fixture shape");
    };
    *groups = Some(SelectionConstraint {
        groups: ["unit-follower", "unit-spell", "unit-follower"]
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let binding = Binding::new(
                    1,
                    ScopeId(10_u64.saturating_add(u64::try_from(index).unwrap())),
                );
                SelectionGroup {
                    selector: Selector::Zone {
                        players: players(PlayerRelation::SelfPlayer),
                        zone: ZoneKind::Deck,
                        view: SelectionView::Current {},
                        binding,
                        predicate: Box::new(name(binding, value)),
                    },
                    cardinality: cardinality(1, 1),
                }
            })
            .collect(),
        cardinality: cardinality(0, 3),
        distinct_by: None,
        disjoint: Disjointness::Disjoint,
        aggregate_bounds: vec![],
    });
    step
}

#[test]
fn groups_are_one_complete_answer_with_empty_entries_and_disjoint_members() {
    for seat in [1, 2] {
        let mut game = game(seat, 4);
        execute(&mut game, seat, grouped());
        let question = game.internal_decision(view(seat)).unwrap();
        let InputDomain::Search(domain) = question.domain else {
            panic!("search");
        };
        let groups = domain.groups.unwrap();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[1].handles.len(), 0);
        for answer in [
            SearchAnswer::Ungrouped(vec![0]),
            SearchAnswer::Grouped(vec![vec![0], vec![]]),
            SearchAnswer::Grouped(vec![vec![0], vec![], vec![], vec![]]),
            SearchAnswer::Grouped(vec![vec![0], vec![], vec![0]]),
            SearchAnswer::Grouped(vec![vec![], vec![0], vec![]]),
            SearchAnswer::Grouped(vec![vec![0, 1], vec![], vec![]]),
        ] {
            let before = exact_authority(&game);
            game.answer_internal(PlayerId { seat }, question.token, Answer::Search(answer))
                .unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
        let result = answer(
            &mut game,
            seat,
            SearchAnswer::Grouped(vec![vec![0], vec![], vec![2]]),
        );
        assert!(result.complete);
        assert_eq!(outcome(&result).found.len(), 2);
        assert_eq!(outcome(&result).moved.len(), 2);
        assert_eq!(shuffles(&result), 1);
    }
}

#[test]
fn failing_final_shuffle_restores_the_execution_and_revokes_the_pending_token() {
    for seat in [1, 2] {
        let position = setup(
            seat,
            json!([{"id":"d0","card":"unit-follower"},{"id":"d1","card":"unit-follower"}]),
        );
        let random = json!({"shuffles":[{"player":seat_name(seat),"result":["foreign"]}]});
        let mut game = Game::new(
            catalog(),
            &position,
            &Value::Null,
            &random,
            "shuffle-rollback",
        )
        .unwrap();
        let origin = game.clone();
        execute(
            &mut game,
            seat,
            search(conditional(), cardinality(1, 1), hand()),
        );
        let token = game.internal_decision(view(seat)).unwrap().token;
        game.answer_internal(
            PlayerId { seat },
            token,
            Answer::Search(SearchAnswer::Ungrouped(vec![0])),
        )
        .unwrap_err();
        super::transactions::assert_restored(&game, &origin);
        assert!(game.internal_decision(view(seat)).is_none());
    }
}

#[test]
fn groups_enforce_global_intersection_total_bounds_and_distinct_names() {
    for seat in [1, 2] {
        for distinct in [false, true] {
            let mut step = grouped();
            let Node::Instruction { instruction } = &mut step.node else {
                panic!("literal fixture shape");
            };
            let Action::Search {
                filter,
                groups: Some(groups),
                cardinality,
                ..
            } = &mut instruction.action
            else {
                panic!("literal fixture shape");
            };
            *filter = conditional();
            if distinct {
                groups.distinct_by = Some(ObjectProperty::Names);
            } else {
                *cardinality = self::cardinality(0, 1);
                groups.cardinality = *cardinality;
            }
            let position = setup(
                seat,
                json!([{"id":"d0","card":"unit-follower"},{"id":"d1","card":"unit-spell"},{"id":"d2","card":"unit-follower"}]),
            );
            let mut game = Game::new(
                catalog(),
                &position,
                &Value::Null,
                &Value::Null,
                "groups-constraints",
            )
            .unwrap();
            execute(&mut game, seat, step);
            let question = game.internal_decision(view(seat)).unwrap();
            let InputDomain::Search(domain) = &question.domain else {
                panic!("search");
            };
            assert_eq!(domain.cards.len(), 2);
            assert_eq!(domain.groups.as_ref().unwrap()[1].handles.len(), 0);
            let before = exact_authority(&game);
            game.answer_internal(
                PlayerId { seat },
                question.token,
                Answer::Search(SearchAnswer::Grouped(vec![vec![0], vec![], vec![1]])),
            )
            .unwrap_err();
            assert_eq!(exact_authority(&game), before);
            assert!(
                answer(
                    &mut game,
                    seat,
                    SearchAnswer::Grouped(vec![vec![0], vec![], vec![]])
                )
                .complete
            );
        }
    }
}

#[cfg(feature = "runner")]
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "Two printed fragments share a public catalogue fixture and exchange both seats."
)]
fn printed_cp04_060_search_and_bp13_035_paid_groups_fragments() {
    use alloc::sync::Arc;
    use std::{env::var_os, fs::read_to_string, path::PathBuf};
    use sve_engine::catalog::Catalog;
    let snapshot = read_to_string(PathBuf::from(
        var_os("SVE_TEST_SNAPSHOT").expect("generate public card input first"),
    ))
    .unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let mut cards = serde_json::Map::new();
    for number in [
        "CP04-060", "CP04-059", "CP04-057", "CP04-071", "BP13-035", "BP13-030", "BP13-032",
        "BP13-033",
    ] {
        cards.insert(
            number.into(),
            json!({"status":"complete","review":"fragment harness only","abilities":[]}),
        );
    }
    let fragments = json!({"format":1,"kind":"effect_set","cards":cards}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(
            &snapshot,
            &registry,
            &[("fragments.yaml".into(), fragments)],
        )
        .unwrap(),
    );
    for seat in [1, 2] {
        // Only the search and movement fragment: UB, evolution, the discount and other abilities stay outside this program.
        let binding = Binding::new(1, ScopeId(2));
        let filter = Expr::Compute(Box::new(Predicate::And {
            predicates: vec![
                Expr::Compute(Box::new(Predicate::HasTrait {
                    traits: Expr::Live(Box::new(TraitsRead::Traits {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::Bound { binding }),
                        },
                        view: CharacteristicView::Effective,
                    })),
                    trait_id: TraitId("ドラゴンズネスト".into()),
                })),
                Expr::Compute(Box::new(Predicate::Not {
                    predicate: name(binding, "カヤ"),
                })),
            ],
        }));
        let mut position = setup(
            seat,
            json!([{"id":"d0","card":"CP04-057"},{"id":"d1","card":"CP04-059"},{"id":"d2","card":"CP04-071"},{"id":"d3","card":"BP13-030"}]),
        );
        position["players"][seat_name(seat)]["zones"]["hand"][0]["card"] = json!("CP04-060");
        position["players"][seat_name(3 - seat)]["zones"]["field"] = json!([]);
        let mut game = Game::new(
            Arc::clone(&catalog),
            &position,
            &Value::Null,
            &Value::Null,
            "printed-search",
        )
        .unwrap();
        let progress = game
            .execute_internal(
                &checked(vec![search(
                    filter,
                    cardinality(1, 1),
                    shared(ZoneKind::Ex),
                )]),
                &source("CP04-060"),
                context(seat),
            )
            .unwrap();
        assert!(!progress.complete);
        let InputDomain::Search(domain) = game.internal_decision(view(seat)).unwrap().domain else {
            panic!("search");
        };
        assert_eq!(
            domain
                .cards
                .iter()
                .map(|card| card.card.as_str())
                .collect::<Vec<_>>(),
            vec!["CP04-057", "CP04-071"]
        );
        let result = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0]));
        assert_eq!(
            outcome(&result).moved[0].actual_to.unwrap().kind,
            ZoneKind::Ex
        );
        assert_eq!(shuffles(&result), 1);

        // The additional cost is already paid; this does not implement its payment, Fusion or the full spell.
        let mut step = grouped();
        let Node::Instruction { instruction } = &mut step.node else {
            panic!("literal fixture shape");
        };
        let Action::Search {
            filter,
            groups: Some(groups),
            ..
        } = &mut instruction.action
        else {
            panic!("literal fixture shape");
        };
        let names = [
            "レヴィオンの副団長・マイム",
            "レヴィオンの魔術師・ミイム",
            "レヴィオンの武闘家・メイム",
        ];
        *filter = Expr::Compute(Box::new(Predicate::Or {
            predicates: names.iter().map(|value| name(binding, value)).collect(),
        }));
        for (group, value) in groups.groups.iter_mut().zip(names) {
            let Selector::Zone {
                binding, predicate, ..
            } = &mut group.selector
            else {
                panic!("literal fixture shape");
            };
            **predicate = name(*binding, value);
        }
        position["players"][seat_name(seat)]["zones"]["hand"][0]["card"] = json!("BP13-035");
        position["players"][seat_name(seat)]["zones"]["deck"] = json!([{"id":"d0","card":"BP13-030"},{"id":"d1","card":"BP13-033"},{"id":"d2","card":"CP04-059"}]);
        let mut game = Game::new(
            Arc::clone(&catalog),
            &position,
            &Value::Null,
            &Value::Null,
            "printed-groups",
        )
        .unwrap();
        game.execute_internal(&checked(vec![step]), &source("BP13-035"), context(seat))
            .unwrap();
        let result = answer(
            &mut game,
            seat,
            SearchAnswer::Grouped(vec![vec![0], vec![], vec![1]]),
        );
        assert!(result.complete);
        assert_eq!(outcome(&result).found.len(), 2);
        assert!(
            outcome(&result)
                .moved
                .iter()
                .all(|movement| movement.actual_to.unwrap().kind == ZoneKind::Field)
        );
        assert_eq!(shuffles(&result), 1);
    }
}

#[test]
fn a_later_failure_rolls_back_the_real_rng_shuffle_with_the_answer() {
    for seat in [1, 2] {
        // Debug output hides the ChaCha state, so the later successful shuffle is the observation.
        let draws = Expr::Compute(Box::new(IntOperation::Arithmetic {
            operation: Arithmetic::Multiply,
            arguments: vec![
                Expr::Compute(Box::new(IntOperation::FromNat {
                    value: Expr::Live(Box::new(NatRead::ZoneCount {
                        selector: super::zone(ZoneKind::Cemetery, 99),
                    })),
                })),
                Expr::Literal(Int(10_001)),
            ],
        }));
        let program = checked(vec![
            search(
                conditional(),
                cardinality(0, 1),
                MoveDestination::Owners {
                    zone: OwnerZone::Cemetery,
                },
            ),
            super::draw_value(9, draws),
        ]);
        let mut games = [game(seat, 10), game(seat, 10)].map(|game| {
            serde_json::from_value::<Game>(serde_json::to_value(game).unwrap()).unwrap()
        });
        let origin = games[0].clone();
        for game in &mut games {
            game.execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
        }
        let token = games[0].internal_decision(view(seat)).unwrap().token;
        games[0]
            .answer_internal(
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![0])),
            )
            .unwrap_err();
        super::transactions::assert_restored(&games[0], &origin);
        games[1] = super::transactions::aligned_control(&games[0], &origin);
        for game in &mut games {
            game.execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
        }
        let results = games
            .each_mut()
            .map(|game| answer(game, seat, SearchAnswer::Ungrouped(vec![])));
        assert!(results[0].complete);
        assert_eq!(results[0], results[1]);
        super::transactions::assert_restored(&games[0], &games[1]);
    }
}

#[test]
fn search_never_puts_a_spell_onto_the_field() {
    for seat in [1, 2] {
        let mut count_only = game(seat, 2);
        let before = exact_authority(&count_only);
        count_only
            .execute_internal(
                &checked(vec![search(
                    Expr::Literal(Bool(true)),
                    cardinality(1, 1),
                    shared(ZoneKind::Field),
                )]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap_err();
        assert_eq!(exact_authority(&count_only), before);

        let mut game = Game::new(
            catalog(),
            &setup(seat, json!([{"id":"d0","card":"unit-spell"}])),
            &Value::Null,
            &Value::Null,
            "spell-field",
        )
        .unwrap();
        execute(
            &mut game,
            seat,
            search(
                name(Binding::new(1, ScopeId(2)), "unit-spell"),
                cardinality(0, 1),
                shared(ZoneKind::Field),
            ),
        );
        let token = game.internal_decision(view(seat)).unwrap().token;
        let before = exact_authority(&game);
        game.answer_internal(
            PlayerId { seat },
            token,
            Answer::Search(SearchAnswer::Ungrouped(vec![0])),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        let result = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![]));
        assert!(result.complete && outcome(&result).moved.is_empty());
        assert_eq!(shuffles(&result), 1);
    }
}

#[test]
fn effective_numeric_reads_survive_a_private_search_input_between_additions() {
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
            "numeric-search",
        )
        .unwrap();
        let modify = |id, amount| {
            instruction(
                id,
                Action::Modify {
                    subjects: Selector::Objects {
                        objects: vec![ObjectOperand::SourceHolder {}],
                    },
                    patch: CharacteristicPatch::Numeric {
                        stat: Stat::Attack,
                        change: NumericChange::Add { amount },
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
        let program = checked(vec![
            modify(9, Expr::Literal(Int(1))),
            search(conditional(), cardinality(0, 1), hand()),
            modify(8, effective_attack),
        ]);
        let pending = game
            .execute_internal(&program, &source("synthetic"), context(seat))
            .unwrap();
        assert!(!pending.complete);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
            3
        );
        let token = game.internal_decision(view(seat)).unwrap().token;
        let before = exact_authority(&game);
        game.answer_internal(
            PlayerId { seat: 3 - seat },
            token,
            Answer::Search(SearchAnswer::Ungrouped(vec![0])),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
        let result = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0]));
        assert!(result.complete);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["power"],
            6
        );
        assert_eq!(outcome(&result).moved.len(), 1);
        assert_eq!(shuffles(&result), 1);
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| matches!(fact, Fact::StatChange(_)))
                .count(),
            2
        );
    }
}

#[path = "search/partition.rs"]
pub(super) mod partition;

#[path = "search/replacement.rs"]
mod replacement;
