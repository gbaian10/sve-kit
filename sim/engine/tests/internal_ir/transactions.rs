//! Invalid payloads preserve a question; accepted answers commit the entire execution.

use super::allocation::{allocated, answer as shares, distribution, leaders};
use super::ordering::{look, looked, order};
use super::search::{cardinality, conditional, hand, search};
use super::*;
use sve_engine::game::internal::{DecisionView, InputDomain, SearchAnswer};

pub(super) fn assert_restored(actual: &Game, expected: &Game) {
    let actual_debug = exact_authority(actual);
    let expected_debug = exact_authority(expected);
    let epoch = |debug: &str| -> u64 {
        debug
            .split_once("ir_epoch: ")
            .unwrap()
            .1
            .split(',')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let actual_epoch = epoch(&actual_debug);
    let expected_epoch = epoch(&expected_debug);
    assert!(
        actual_epoch >= expected_epoch,
        "rollback must retain the epoch high water mark"
    );
    assert_eq!(
        actual_debug,
        expected_debug.replacen(
            &format!("ir_epoch: {expected_epoch},"),
            &format!("ir_epoch: {actual_epoch},"),
            1
        ),
        "rollback must restore every authority field except the epoch high water mark",
    );
    match serde_json::to_value(expected) {
        Ok(before) => {
            let mut after = serde_json::to_value(actual).unwrap();
            after["state"]["ir_epoch"] = before["state"]["ir_epoch"].clone();
            assert_eq!(after, before, "saved RNG and authority must match");
        }
        Err(_) => {
            assert!(
                serde_json::to_value(actual).is_err(),
                "both sides must reject saves; RNG state is unavailable for this comparison"
            );
        }
    }
    for view in [View::P1, View::P2, View::Referee] {
        assert_eq!(
            actual.projection(view).unwrap(),
            expected.projection(view).unwrap(),
            "all player observations must match",
        );
    }
}

pub(super) fn aligned_control(actual: &Game, pristine: &Game) -> Game {
    let mut control = serde_json::to_value(pristine).unwrap();
    control["state"]["ir_epoch"] =
        serde_json::to_value(actual).unwrap()["state"]["ir_epoch"].clone();
    serde_json::from_value(control).unwrap()
}

#[expect(
    clippy::panic,
    reason = "Unexpected fixture domains must fail the test."
)]
fn response(question: &DecisionView) -> Answer {
    match &question.domain {
        InputDomain::BeforeDraw => Answer::Stepwise(StepwiseAnswer::Continue),
        InputDomain::Distribution(_) => shares(&[(0, 3), (1, 1)]),
        InputDomain::Order(cards) => {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        }
        InputDomain::Search(_) => Answer::Search(SearchAnswer::Ungrouped(vec![0, 1])),
        InputDomain::SearchPartition(domain) => {
            Answer::SearchPartition(vec![domain.cards[0].handle])
        }
        InputDomain::TokenCapacity { .. }
        | InputDomain::MealPayment { .. }
        | InputDomain::WardEntry { .. }
        | InputDomain::Integer(_)
        | InputDomain::SearchReplacement
        | InputDomain::Targets(_)
        | InputDomain::Pending(_)
        | InputDomain::TriggerQuota(_)
        | InputDomain::Modes { .. }
        | InputDomain::Quick { .. }
        | InputDomain::OptionalCost
        | InputDomain::CostMaterials(_) => {
            panic!("unexpected transaction fixture question")
        }
    }
}

#[expect(clippy::panic, reason = "Unknown fixture indices must fail the test.")]
fn interactive(kind: usize) -> Vec<Located<Node>> {
    match kind {
        0 => vec![search(conditional(), cardinality(2, 2), hand())],
        1 => vec![search::partition::partition(
            cardinality(1, 1),
            cardinality(1, 1),
        )],
        2 => vec![look(10, 3), order(11, looked(10), Position::Bottom {})],
        3 => vec![up_to(12, 2)],
        _ => panic!("unknown fixture"),
    }
}

fn prefix() -> Located<Node> {
    instruction(
        80,
        Action::Modify {
            subjects: zone(ZoneKind::Field, 80),
            patch: CharacteristicPatch::Numeric {
                stat: Stat::Attack,
                change: NumericChange::Add {
                    amount: Expr::Literal(Int(2)),
                },
            },
            expires: Duration {
                end: DurationEnd::EndResolution {},
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            source: EffectReference::CurrentInstallation {},
        },
    )
}

#[test]
fn every_interaction_rolls_back_all_prior_answers_and_never_revives_tokens() {
    for seat in [1, 2] {
        for kind in 0..4 {
            for distribution_first in [false, true] {
                let mut game = game(seat, 10);
                let origin = game.clone();
                let mut steps = vec![prefix(), draw(70, 1)];
                let distribution = vec![
                    distribution(60, leaders(), Expr::Literal(Nat(4)), 1),
                    allocated(61, 60, leaders()),
                ];
                if distribution_first {
                    steps.extend(distribution.clone());
                }
                steps.extend(interactive(kind));
                if !distribution_first {
                    steps.extend(distribution);
                }
                steps.push(draw(90, 10_001));
                let program = checked(steps);
                game.execute_internal(&program, &source("synthetic"), context(seat))
                    .unwrap();
                let mut old = Vec::new();
                loop {
                    let question = game.internal_decision(View::Referee).unwrap();
                    let before = exact_authority(&game);
                    game.answer_internal(question.actor, question.token, Answer::Integer(Nat(0)))
                        .unwrap_err();
                    assert_eq!(exact_authority(&game), before);
                    assert_eq!(
                        game.internal_decision(View::Referee),
                        Some(question.clone())
                    );
                    let answer = response(&question);
                    old.push((question.clone(), answer.clone()));
                    match game.answer_internal(question.actor, question.token, answer) {
                        Ok(progress) => assert!(!progress.complete),
                        Err(error) => {
                            assert!(
                                error
                                    .to_string()
                                    .contains("draw request exceeds internal execution limit"),
                                "{error}"
                            );
                            break;
                        }
                    }
                }
                assert_restored(&game, &origin);
                game.execute_internal(&program, &source("synthetic"), context(seat))
                    .unwrap();
                let fresh = game.internal_decision(View::Referee).unwrap();
                assert!(
                    old.iter()
                        .all(|(question, _)| fresh.token.epoch > question.token.epoch)
                );
                for (question, answer) in old {
                    let before = exact_authority(&game);
                    let error = game
                        .answer_internal(question.actor, question.token, answer)
                        .unwrap_err();
                    assert!(
                        error.to_string().contains("stale internal answer"),
                        "{error}"
                    );
                    assert_eq!(exact_authority(&game), before);
                }
            }
        }
    }
}

#[test]
fn every_mixed_interaction_can_complete_and_clean_up_resolution_effects() {
    for seat in [1, 2] {
        for kind in 0..4 {
            for distribution_first in [false, true] {
                let mut game = game(seat, 10);
                let mut steps = vec![prefix()];
                let distribution = vec![
                    distribution(60, leaders(), Expr::Literal(Nat(4)), 1),
                    allocated(61, 60, leaders()),
                ];
                if distribution_first {
                    steps.extend(distribution.clone());
                }
                steps.extend(interactive(kind));
                if !distribution_first {
                    steps.extend(distribution);
                }
                game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
                    .unwrap();
                let progress = loop {
                    let question = game.internal_decision(View::Referee).unwrap();
                    let result = game
                        .answer_internal(question.actor, question.token, response(&question))
                        .unwrap();
                    if result.complete {
                        break result;
                    }
                };
                assert!(game.internal_decision(View::Referee).is_none());
                assert!(matches!(
                    progress.receipts[&ReceiptId(61)],
                    InstructionOutcome::Damage(DamageQuantityOutcome {
                        requested: Int(4),
                        actual: Nat(4)
                    })
                ));
                let projection = game.projection(View::Referee).unwrap();
                assert_eq!(projection["objects"]["f"]["power"], 2);
            }
        }
    }
}

#[test]
fn draw_mutation_and_next_question_failures_restore_the_origin() {
    for seat in [1, 2] {
        for next_question in [false, true] {
            let original = game(seat, 3);
            let mut saved = serde_json::to_value(&original).unwrap();
            if next_question {
                saved["state"]["ir_epoch"] = json!(u64::MAX - 3);
            } else {
                saved["state"]["objects"]["d0"]["generation"] = json!(u64::MAX);
            }
            let mut game: Game = serde_json::from_value(saved).unwrap();
            let origin = game.clone();
            game.execute_internal(
                &checked(vec![up_to(1, 2)]),
                &source("synthetic"),
                context(seat),
            )
            .unwrap();
            let question = game.internal_decision(View::Referee).unwrap();
            let error = game
                .answer_internal(question.actor, question.token, response(&question))
                .unwrap_err();
            assert!(
                error.to_string().contains(if next_question {
                    "epoch exhausted"
                } else {
                    "generation"
                }),
                "{error}"
            );
            assert_restored(&game, &origin);
        }
    }
}

#[test]
fn search_and_partition_mutation_errors_revoke_the_execution() {
    for seat in [1, 2] {
        for partition in [false, true] {
            let original = game(seat, 3);
            let mut saved = serde_json::to_value(&original).unwrap();
            saved["state"]["objects"]["d0"]["generation"] = json!(u64::MAX);
            let mut game: Game = serde_json::from_value(saved).unwrap();
            let origin = game.clone();
            let program = checked(interactive(usize::from(partition)));
            game.execute_internal(&program, &source("synthetic"), context(seat))
                .unwrap();
            let question = game.internal_decision(View::Referee).unwrap();
            let answer = response(&question);
            let result = game.answer_internal(question.actor, question.token, answer);
            let error = if partition {
                result.unwrap();
                let question = game.internal_decision(View::Referee).unwrap();
                game.answer_internal(question.actor, question.token, response(&question))
                    .unwrap_err()
            } else {
                result.unwrap_err()
            };
            assert!(error.to_string().contains("generation"), "{error}");
            assert_restored(&game, &origin);
        }
    }
}

#[test]
fn search_order_and_partition_next_question_errors_are_commit_failures() {
    for seat in [1, 2] {
        for kind in 0..3 {
            let original = game(seat, 5);
            let mut saved = serde_json::to_value(&original).unwrap();
            saved["state"]["ir_epoch"] = json!(
                u64::MAX
                    - match kind {
                        0 | 1 => 4,
                        _ => 5,
                    }
            );
            let mut game: Game = serde_json::from_value(saved).unwrap();
            let origin = game.clone();
            let mut steps = interactive(kind);
            steps.push(up_to(90, 1));
            game.execute_internal(&checked(steps), &source("synthetic"), context(seat))
                .unwrap();
            loop {
                let question = game.internal_decision(View::Referee).unwrap();
                match game.answer_internal(question.actor, question.token, response(&question)) {
                    Ok(progress) => assert!(!progress.complete),
                    Err(error) => {
                        assert!(error.to_string().contains("epoch exhausted"), "{error}");
                        break;
                    }
                }
            }
            assert_restored(&game, &origin);
        }
    }
}
