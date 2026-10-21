//! BP03-002's found-local cost/destination fragment, with synthetic boundary positions.

use super::*;

fn cost_condition() -> Expr<Bool> {
    cost_bound(CharacteristicView::Effective, Comparison::LessEqual)
}
fn cost_bound(view: CharacteristicView, comparison: Comparison) -> Expr<Bool> {
    let object = Binding::new(1, ScopeId(4));
    Expr::Compute(Box::new(Predicate::SearchFoundAny {
        found: Binding::new(2, ScopeId(3)),
        object,
        predicate: Expr::Compute(Box::new(Predicate::CompareInt {
            left: Expr::Compute(Box::new(IntOperation::InformationOrZero {
                information: NumberRead::Cost {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::Bound { binding: object }),
                    },
                    view,
                },
            })),
            comparison,
            right: Expr::Literal(Int(2)),
        })),
    }))
}
fn replacement(predicate: Expr<Bool>) -> Located<Node> {
    let mut step = search(conditional(), cardinality(1, 1), hand());
    if let Node::Instruction { instruction } = &mut step.node
        && let Action::Search { destination, .. } = &mut instruction.action
    {
        *destination = Some(SearchDestination::OptionalSelfReplacement {
            decision: DecisionId(11),
            predicate,
            movement: movement(shared(ZoneKind::Field)),
        });
    }
    step
}
fn choose(game: &mut Game, seat: u32, choice: SearchReplacementAnswer) -> Progress {
    let question = game.internal_decision(view(seat)).unwrap();
    assert_eq!(
        question.domain,
        InputDomain::SearchReplacement,
        "the answer is only a destination choice"
    );
    game.answer_internal(
        PlayerId { seat },
        question.token,
        Answer::SearchReplacement(choice),
    )
    .unwrap()
}
fn position(seat: u32, count: usize, cost: i64, field: usize) -> Game {
    let deck = (0..count)
        .map(|index| json!({"id":format!("d{index}"),"card":"unit-follower","state":{"cost":cost}}))
        .collect::<Vec<_>>();
    let mut position = setup(seat, json!(deck));
    position["players"][seat_name(seat)]["zones"]["field"] = json!(
        (0..field)
            .map(|index| json!({"id":format!("own{index}"),"card":"unit-follower"}))
            .collect::<Vec<_>>()
    );
    Game::new(catalog(), &position, &Value::Null, &Value::Null, "D1-seed").unwrap()
}

#[test]
#[expect(
    clippy::cognitive_complexity,
    reason = "Both seats and destinations share the same proof, token, receipt and observation trace."
)]
fn found_is_proved_before_the_new_question_and_both_choices_shuffle_once() {
    for seat in [1, 2] {
        for choice in [
            SearchReplacementAnswer::Original,
            SearchReplacementAnswer::Replacement,
        ] {
            let mut game = game(seat, 3);
            execute(&mut game, seat, replacement(cost_condition()));
            let search_token = game.internal_decision(view(seat)).unwrap().token;
            let progress = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![1]));
            assert!(
                !progress.complete && progress.facts.is_empty() && progress.receipts.is_empty()
            );
            let question = game.internal_decision(view(seat)).unwrap();
            assert_eq!(question.domain, InputDomain::SearchReplacement);
            assert_eq!(question.token.decision, DecisionId(11));
            assert_ne!(question.token, search_token);
            assert!(question.token.epoch > search_token.epoch);
            assert_eq!(game.internal_decision(View::Referee).unwrap(), question);
            assert!(game.internal_decision(view(3 - seat)).is_none());
            assert_eq!(
                game.projection(view(seat)).unwrap()["awaiting"]["domain"],
                json!({"kind":"search-replacement","options":["original","replacement"]}),
                "the searcher's packet carries no cards or continuation"
            );
            let public = game.projection(view(3 - seat)).unwrap();
            assert!(public["awaiting"].is_null());
            assert!(
                public["known_cards"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|card| card["id"] == "d1" && card["card"] == "unit-follower")
            );
            assert!(!public.to_string().contains("d0") && !public.to_string().contains("d2"));
            assert_eq!(
                game.projection(View::Referee).unwrap()[seat_name(seat)]["deck"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
            let result = choose(&mut game, seat, choice);
            assert!(result.complete);
            let result_outcome = outcome(&result);
            assert_eq!(result_outcome.found, result_outcome.revealed);
            assert_eq!(result_outcome.found.len(), 1);
            let moved = &result_outcome.moved[0];
            assert_eq!(moved.before, result_outcome.found[0].object);
            assert_eq!(moved.before.generation, Generation(0));
            assert_eq!(moved.after.unwrap().generation, Generation(1));
            assert_eq!(
                moved.actual_to.unwrap().kind,
                if choice == SearchReplacementAnswer::Original {
                    ZoneKind::Hand
                } else {
                    ZoneKind::Field
                }
            );
            assert_eq!(moved.actual_to, Some(moved.requested_to));
            assert_eq!(result.facts.len(), 2);
            assert_eq!(shuffles(&result), 1);
            if choice == SearchReplacementAnswer::Original {
                assert!(
                    game.projection(view(3 - seat)).unwrap()["objects"]
                        .get("d1")
                        .is_none()
                );
            }
        }
    }
}

#[test]
fn zero_found_and_false_cost_skip_replacement_even_when_the_field_is_full() {
    for seat in [1, 2] {
        for (count, cost, selected) in [(0, 1, vec![]), (1, 1, vec![]), (1, 3, vec![0])] {
            let mut game = position(seat, count, cost, 5);
            execute(&mut game, seat, replacement(cost_condition()));
            let result = answer(&mut game, seat, SearchAnswer::Ungrouped(selected.clone()));
            assert!(result.complete && game.internal_decision(view(seat)).is_none());
            assert_eq!(outcome(&result).found.len(), selected.len());
            assert_eq!(outcome(&result).moved.len(), selected.len());
            assert!(
                outcome(&result)
                    .moved
                    .iter()
                    .all(|result| result.actual_to.unwrap().kind == ZoneKind::Hand)
            );
            assert_eq!(shuffles(&result), 1);
        }
        let mut game = game(seat, 1);
        execute(&mut game, seat, replacement(Expr::Literal(Bool(false))));
        let result = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0]));
        assert!(result.complete);
        assert_eq!(
            outcome(&result).moved[0].actual_to.unwrap().kind,
            ZoneKind::Hand
        );
    }
}

#[test]
fn effective_cost_boundary_controls_the_question_without_changing_found() {
    for seat in [1, 2] {
        for cost in [-1, 0, 2, 3] {
            let mut game = position(seat, 1, cost, 4);
            execute(&mut game, seat, replacement(cost_condition()));
            let progress = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0]));
            let result = if cost <= 2 {
                assert!(!progress.complete);
                choose(&mut game, seat, SearchReplacementAnswer::Replacement)
            } else {
                progress
            };
            assert!(result.complete);
            assert_eq!(outcome(&result).found.len(), 1);
            assert_eq!(
                outcome(&result).moved[0].actual_to.unwrap().kind,
                if cost <= 2 {
                    ZoneKind::Field
                } else {
                    ZoneKind::Hand
                }
            );
            assert_eq!(shuffles(&result), 1);
        }
    }
}

#[test]
fn full_field_rejects_only_an_actual_optional_choice_and_restores_the_search() {
    for seat in [1, 2] {
        let mut game = position(seat, 2, 1, 5);
        execute(&mut game, seat, replacement(cost_condition()));
        let before = exact_authority(&game);
        let token = game.internal_decision(view(seat)).unwrap().token;
        assert!(matches!(
            game.answer_internal(
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![0]))
            ),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(exact_authority(&game), before);
        let result = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![]));
        assert!(result.complete);
        assert_eq!(shuffles(&result), 1);
    }
}

#[test]
fn replacement_rejects_wrong_actor_tokens_variants_and_replay_atomically() {
    for seat in [1, 2] {
        let mut game = game(seat, 3);
        execute(&mut game, seat, replacement(cost_condition()));
        let old = game.internal_decision(view(seat)).unwrap().token;
        answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0]));
        let current = game.internal_decision(view(seat)).unwrap().token;
        for (actor, token, payload) in [
            (
                3 - seat,
                current,
                Answer::SearchReplacement(SearchReplacementAnswer::Original),
            ),
            (
                seat,
                old,
                Answer::SearchReplacement(SearchReplacementAnswer::Replacement),
            ),
            (
                seat,
                DecisionToken {
                    epoch: current.epoch + 1,
                    ..current
                },
                Answer::SearchReplacement(SearchReplacementAnswer::Original),
            ),
            (
                seat,
                DecisionToken {
                    decision: DecisionId(999),
                    ..current
                },
                Answer::SearchReplacement(SearchReplacementAnswer::Original),
            ),
            (
                seat,
                DecisionToken {
                    execution: ExecutionId(current.execution.0 + 1),
                    ..current
                },
                Answer::SearchReplacement(SearchReplacementAnswer::Original),
            ),
            (
                seat,
                current,
                Answer::Search(SearchAnswer::Ungrouped(vec![1])),
            ),
            (seat, current, Answer::SearchPartition(vec![0])),
        ] {
            let before = exact_authority(&game);
            game.answer_internal(PlayerId { seat: actor }, token, payload)
                .unwrap_err();
            assert_eq!(exact_authority(&game), before);
        }
        choose(&mut game, seat, SearchReplacementAnswer::Original);
        let before = exact_authority(&game);
        game.answer_internal(
            PlayerId { seat },
            current,
            Answer::SearchReplacement(SearchReplacementAnswer::Original),
        )
        .unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn failure_after_choice_and_shuffle_restores_execution_and_real_rng() {
    for seat in [1, 2] {
        let count = Expr::Compute(Box::new(IntOperation::Conditional {
            condition: Expr::Compute(Box::new(Predicate::CompareNat {
                left: Expr::Live(Box::new(NatRead::ZoneCount {
                    selector: super::super::zone(ZoneKind::Field, 99),
                })),
                comparison: Comparison::Greater,
                right: Expr::Literal(Nat(1)),
            })),
            then_value: Expr::Literal(Int(10_001)),
            else_value: Expr::Literal(Int(0)),
        }));
        let program = checked(vec![
            replacement(cost_condition()),
            super::super::draw_value(9, count),
        ]);
        let mut games = [game(seat, 10), game(seat, 10)].map(|game| {
            serde_json::from_value::<Game>(serde_json::to_value(game).unwrap()).unwrap()
        });
        let origin = games[0].clone();
        for game in &mut games {
            game.execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
            answer(game, seat, SearchAnswer::Ungrouped(vec![0]));
        }
        let token = games[0].internal_decision(view(seat)).unwrap().token;
        games[0]
            .answer_internal(
                PlayerId { seat },
                token,
                Answer::SearchReplacement(SearchReplacementAnswer::Replacement),
            )
            .unwrap_err();
        super::super::transactions::assert_restored(&games[0], &origin);
        games[1] = super::super::transactions::aligned_control(&games[0], &origin);
        for game in &mut games {
            game.execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
            answer(game, seat, SearchAnswer::Ungrouped(vec![0]));
        }
        let results = games
            .each_mut()
            .map(|game| choose(game, seat, SearchReplacementAnswer::Original));
        assert!(results[0].complete);
        assert_eq!(results[0], results[1]);
        super::super::transactions::assert_restored(&games[0], &games[1]);
    }
}

#[test]
fn unsupported_replacements_are_rejected_in_dead_branches_before_an_earlier_draw() {
    for seat in [1, 2] {
        for case in 0..9 {
            let mut step = replacement(cost_condition());
            if let Node::Instruction { instruction } = &mut step.node
                && let Action::Search {
                    filter,
                    cardinality: bounds,
                    destination:
                        Some(SearchDestination::OptionalSelfReplacement {
                            predicate,
                            movement,
                            ..
                        }),
                    ..
                } = &mut instruction.action
            {
                match case {
                    0 => {}
                    1 => *bounds = cardinality(1, 2),
                    2 => bounds.maximum = None,
                    3 => {
                        movement.to = MoveDestination::Owners {
                            zone: OwnerZone::Cemetery,
                        }
                    }
                    4 => {
                        *predicate = Expr::Compute(Box::new(Predicate::SearchFoundAny {
                            found: Binding::new(2, ScopeId(3)),
                            object: Binding::new(1, ScopeId(4)),
                            predicate: Expr::Literal(Bool(true)),
                        }));
                    }
                    5 => {
                        *predicate =
                            cost_bound(CharacteristicView::Original, Comparison::LessEqual);
                    }
                    6 => *predicate = cost_bound(CharacteristicView::Effective, Comparison::Less),
                    // Without a printed condition nothing is revealed before the private cost read.
                    7 => *filter = Expr::Literal(Bool(true)),
                    _ => movement.initial_posture = Some(Posture::Acted),
                }
            }
            let program = checked(vec![
                super::super::draw(9, 1),
                node(Node::Control {
                    control: Control::If {
                        condition: Expr::Literal(Bool(false)),
                        then_body: body(8, vec![step]),
                        else_body: None,
                    },
                }),
            ]);
            let mut game = game(seat, 3);
            let before = exact_authority(&game);
            let result = game.execute_internal(&program, &source("synthetic"), context(seat));
            if case == 0 {
                assert!(result.unwrap().complete);
                assert_ne!(exact_authority(&game), before);
            } else {
                assert!(matches!(result, Err(EngineFailure::Unsupported(_))));
                assert_eq!(exact_authority(&game), before);
            }
        }
    }
}

#[test]
fn cost_reads_remain_rejected_outside_the_search_destination() {
    let cost = Expr::Compute(Box::new(IntOperation::InformationOrZero {
        information: NumberRead::Cost {
            object: ObjectView::Current {
                object: Box::new(ObjectOperand::SourceHolder {}),
            },
            view: CharacteristicView::Effective,
        },
    }));
    for seat in [1, 2] {
        let mut game = game(seat, 3);
        let before = exact_authority(&game);
        let program = checked(vec![
            super::super::draw(8, 1),
            super::super::draw_value(9, cost.clone()),
        ]);
        assert!(matches!(
            game.execute_internal(&program, &source("synthetic"), context(seat)),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn a_found_spell_cannot_acquire_an_optional_field_destination() {
    for seat in [1, 2] {
        let mut game = Game::new(
            catalog(),
            &setup(seat, json!([{"id":"d0","card":"unit-spell"}])),
            &Value::Null,
            &Value::Null,
            "spell-boundary",
        )
        .unwrap();
        let mut step = replacement(cost_condition());
        if let Node::Instruction { instruction } = &mut step.node
            && let Action::Search { filter, .. } = &mut instruction.action
        {
            *filter = name(Binding::new(1, ScopeId(2)), "unit-spell");
        }
        execute(&mut game, seat, step);
        let before = exact_authority(&game);
        let token = game.internal_decision(view(seat)).unwrap().token;
        assert!(matches!(
            game.answer_internal(
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![0]))
            ),
            Err(EngineFailure::Unsupported(_))
        ));
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn printed_bp03_002_fragment_uses_the_public_beast_traits_and_costs() {
    use alloc::sync::Arc;
    use std::{env::var_os, fs::read_to_string, path::PathBuf};
    use sve_engine::catalog::Catalog;
    let snapshot = read_to_string(PathBuf::from(
        var_os("SVE_TEST_SNAPSHOT").expect("generate public card input first"),
    ))
    .unwrap();
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let mut cards = serde_json::Map::new();
    for number in ["BP03-002", "BP03-009", "BP03-001"] {
        cards.insert(
            number.into(),
            json!({"status":"complete","review":"fragment harness only","abilities":[]}),
        );
    }
    let document = json!({"format":1,"kind":"effect_set","cards":cards}).to_string();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("fragments.yaml".into(), document)])
            .unwrap(),
    );
    for seat in [1, 2] {
        for (number, expected) in [("BP03-009", ZoneKind::Field), ("BP03-001", ZoneKind::Hand)] {
            let mut position = setup(seat, json!([{"id":"d0","card":number}]));
            position["players"][seat_name(seat)]["zones"]["hand"][0]["card"] = json!("BP03-002");
            position["players"][seat_name(3 - seat)]["zones"]["field"][0]["card"] =
                json!("BP03-001");
            let mut game = Game::new(
                Arc::clone(&catalog),
                &position,
                &Value::Null,
                &Value::Null,
                "BP03-002-fragment",
            )
            .unwrap();
            let mut step = replacement(cost_condition());
            if let Node::Instruction { instruction } = &mut step.node
                && let Action::Search { filter, .. } = &mut instruction.action
            {
                *filter = Expr::Compute(Box::new(Predicate::HasTrait {
                    traits: Expr::Live(Box::new(TraitsRead::Traits {
                        object: ObjectView::Current {
                            object: Box::new(ObjectOperand::Bound {
                                binding: Binding::new(1, ScopeId(2)),
                            }),
                        },
                        view: CharacteristicView::Effective,
                    })),
                    trait_id: TraitId("獣".into()),
                }));
            }
            game.execute_internal(&checked(vec![step]), &source("BP03-002"), context(seat))
                .unwrap();
            let found = answer(&mut game, seat, SearchAnswer::Ungrouped(vec![0]));
            let result = if expected == ZoneKind::Field {
                choose(&mut game, seat, SearchReplacementAnswer::Replacement)
            } else {
                found
            };
            assert!(result.complete);
            assert_eq!(outcome(&result).found.len(), 1);
            assert_eq!(outcome(&result).revealed, outcome(&result).found);
            assert_eq!(outcome(&result).moved[0].actual_to.unwrap().kind, expected);
            assert_eq!(shuffles(&result), 1);
        }
    }
}

#[test]
fn malformed_printed_costs_are_rejected_instead_of_reading_as_no_cost() {
    use alloc::sync::Arc;
    use sve_engine::catalog::Catalog;
    let registry = json!({"format":1,"kind":"keyword_registry","keywords":{}}).to_string();
    let document = json!({"format":1,"kind":"effect_set","cards":{
        "unit-spell":{"status":"complete","review":"synthetic","abilities":[]},
        "unit-follower":{"status":"complete","review":"synthetic","abilities":[]}
    }})
    .to_string();
    for seat in [1, 2] {
        for (cost, offered) in [
            (json!("9223372036854775808"), None),
            (json!("2a"), None),
            (json!(2.0), None),
            (json!("-"), Some(true)),
            (Value::Null, Some(true)),
            (json!("3"), Some(false)),
        ] {
            let snapshot = [("unit-spell", "スペル", json!("1")), ("unit-follower", "フォロワー", cost)]
                .into_iter()
                .map(|(number, kind, cost)| json!({"number":number,"faces":[{"name":number,"card_class":"ニュートラル","card_type":kind,"cost":cost,"power":"2","hp":"3","traits":[],"text":null,"sections":[]}]}).to_string())
                .collect::<Vec<_>>()
                .join("\n");
            let catalog = Arc::new(
                Catalog::from_documents(
                    &snapshot,
                    &registry,
                    &[("unit.yaml".into(), document.clone())],
                )
                .unwrap(),
            );
            let mut game = Game::new(
                catalog,
                &setup(seat, json!([{"id":"d0","card":"unit-follower"}])),
                &Value::Null,
                &Value::Null,
                "printed-cost",
            )
            .unwrap();
            execute(&mut game, seat, replacement(cost_condition()));
            let token = game.internal_decision(view(seat)).unwrap().token;
            let before = exact_authority(&game);
            let result = game.answer_internal(
                PlayerId { seat },
                token,
                Answer::Search(SearchAnswer::Ungrouped(vec![0])),
            );
            match offered {
                None => {
                    assert!(matches!(result, Err(EngineFailure::Invalid(_))));
                    assert_eq!(exact_authority(&game), before);
                    assert!(answer(&mut game, seat, SearchAnswer::Ungrouped(vec![])).complete);
                }
                Some(true) => {
                    assert!(!result.unwrap().complete);
                    let done = choose(&mut game, seat, SearchReplacementAnswer::Replacement);
                    assert_eq!(
                        outcome(&done).moved[0].actual_to.unwrap().kind,
                        ZoneKind::Field
                    );
                }
                Some(false) => {
                    let done = result.unwrap();
                    assert!(done.complete);
                    assert_eq!(
                        outcome(&done).moved[0].actual_to.unwrap().kind,
                        ZoneKind::Hand
                    );
                }
            }
        }
    }
}

#[test]
fn optional_replacement_can_follow_and_precede_distribution() {
    for seat in [1, 2] {
        for first in [false, true] {
            let distribution = super::super::allocation::distribution(
                20,
                super::super::allocation::leaders(),
                Expr::Literal(Nat(4)),
                1,
            );
            let mut steps = vec![replacement(cost_condition())];
            if first {
                steps.insert(0, distribution);
            } else {
                steps.push(distribution);
            }
            let mut game = game(seat, 3);
            game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
                .unwrap();
            loop {
                let question = game.internal_decision(View::Referee).unwrap();
                let answer = match question.domain {
                    InputDomain::Distribution(_) => {
                        super::super::allocation::answer(&[(0, 3), (1, 1)])
                    }
                    InputDomain::Search(_) => Answer::Search(SearchAnswer::Ungrouped(vec![0])),
                    InputDomain::SearchReplacement => {
                        Answer::SearchReplacement(SearchReplacementAnswer::Original)
                    }
                    InputDomain::BeforeDraw
                    | InputDomain::Integer(_)
                    | InputDomain::Order(_)
                    | InputDomain::SearchPartition(_) => panic!("unexpected question"),
                };
                let progress = game
                    .answer_internal(question.actor, question.token, answer)
                    .unwrap();
                if progress.complete {
                    assert_eq!(shuffles(&progress), 1);
                    break;
                }
            }
        }
    }
}
