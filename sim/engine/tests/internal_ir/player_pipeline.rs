#![expect(
    clippy::panic,
    reason = "Unexpected public input shapes fail the fixture."
)]

use super::*;
use sve_engine::game::player::{
    Command, Input, MainActionAnswer, MainIntentKind, Question, QuestionShape, Submission,
    SubmissionKind, Viewer,
};

pub(super) fn question(game: &Game, viewer: Viewer) -> Question {
    let Input::Question(question) = game.input(viewer) else {
        panic!("expected a player question");
    };
    *question
}

pub(super) fn answer(game: &mut Game, viewer: Viewer, answer: Answer) -> Submission {
    let question = question(game, viewer);
    game.submit_player(
        viewer,
        Command::Answer {
            token: question.token,
            answer,
        },
    )
}

pub(super) fn end_command(question: &Question) -> Command {
    let QuestionShape::ActionWindow(actions) = &question.shape else {
        panic!("main actions");
    };
    let end = actions
        .intents
        .iter()
        .find(|intent| matches!(intent.kind, MainIntentKind::EndPhase))
        .unwrap();
    Command::MainAction {
        token: question.token,
        intent: end.id.clone(),
        answer: MainActionAnswer::None,
    }
}

#[test]
fn player_pipeline_reads_and_rejections_preserve_the_question_and_rng() {
    let mut game = game(1, 3);
    game.execute_internal(
        &checked(vec![up_to(1, 2)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    game.advance_player_input();
    let first = question(&game, Viewer::P1);
    assert!(!format!("{:?}", first.token).contains("epoch"));
    assert!(!format!("{:?}", first.token).contains("serial"));
    let before = exact_authority(&game);
    for _ in 0..10 {
        assert_eq!(question(&game, Viewer::P1), first);
        assert!(matches!(game.input(Viewer::P2), Input::Waiting(summary)
            if summary.actor == Viewer::P1 && summary.dice.is_none()));
    }
    for (viewer, payload) in [
        (Viewer::P2, Answer::Stepwise(StepwiseAnswer::Continue)),
        (Viewer::P1, Answer::Modes(vec![])),
    ] {
        let result = game.submit_player(
            viewer,
            Command::Answer {
                token: first.token,
                answer: payload,
            },
        );
        assert_eq!(result.kind, SubmissionKind::Rejected);
        assert_eq!(question(&game, Viewer::P1), first);
        assert_eq!(exact_authority(&game), before);
    }
    assert_eq!(
        answer(
            &mut game,
            Viewer::P1,
            Answer::Stepwise(StepwiseAnswer::Continue)
        )
        .kind,
        SubmissionKind::Applied
    );
    let second = question(&game, Viewer::P1);
    assert_ne!(second.token, first.token);
    let before_stale = exact_authority(&game);
    let stale = game.submit_player(
        Viewer::P1,
        Command::Answer {
            token: first.token,
            answer: Answer::Stepwise(StepwiseAnswer::Stop),
        },
    );
    assert_eq!(stale.kind, SubmissionKind::Rejected);
    assert_eq!(question(&game, Viewer::P1), second);
    assert_eq!(exact_authority(&game), before_stale);
    let result = answer(
        &mut game,
        Viewer::P1,
        Answer::Stepwise(StepwiseAnswer::Stop),
    );
    assert_eq!(result.kind, SubmissionKind::Applied);
    assert!(matches!(
        question(&game, Viewer::P1).shape,
        QuestionShape::ActionWindow(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["s", "d0"])
    );
}

#[test]
fn player_pipeline_fault_rolls_back_the_play_without_a_rules_defeat() {
    let ep = |id, amount| {
        instruction(
            id,
            Action::Resource {
                player: PlayerRef::Controller {},
                resource: Resource::Ep,
                operation: ResourceOperation::Add,
                amount: Expr::Literal(Int(amount)),
            },
        )
    };
    for tail in [vec![draw(3, 10_001)], vec![ep(3, i64::MAX), ep(4, 1)]] {
        let mut game = game(1, 3);
        let origin = game.clone();
        let mut steps = vec![draw(1, 1), up_to(2, 1)];
        steps.extend(tail);
        game.execute_internal(&checked(steps), &source("synthetic"), context(1))
            .unwrap();
        let failed = answer(
            &mut game,
            Viewer::P1,
            Answer::Stepwise(StepwiseAnswer::Stop),
        );
        assert_eq!(failed.kind, SubmissionKind::EngineFault);
        assert_eq!(failed.input, Input::EngineFault);
        assert_eq!(game.input(Viewer::P2), Input::EngineFault);
        let mut expected = serde_json::to_value(&origin).unwrap();
        let mut restored = serde_json::to_value(&game).unwrap();
        assert!(
            restored["state"]["ir_epoch"].as_u64().unwrap()
                >= expected["state"]["ir_epoch"].as_u64().unwrap()
        );
        restored["state"]["ir_epoch"] = expected["state"]["ir_epoch"].clone();
        transactions::normalize_saved_issuance(&mut restored, &mut expected);
        assert_eq!(
            restored, expected,
            "all authority and RNG data must roll back"
        );
        for viewer in [Viewer::P1, Viewer::P2] {
            assert_eq!(
                game.observe(viewer).unwrap(),
                origin.observe(viewer).unwrap()
            );
        }
        assert!(!format!("{failed:?}").contains("draw request"));
        assert!(!format!("{failed:?}").contains("integer representation"));
        game.advance_player_input();
        assert_eq!(game.input(Viewer::P1), Input::EngineFault);
        assert_ne!(
            game.projection(View::Referee).unwrap()["game"]["ended"],
            true
        );
    }
}

#[test]
fn player_pipeline_waiting_opponent_can_concede_without_a_token() {
    let mut game = game(1, 3);
    game.execute_internal(
        &checked(vec![draw(1, 1), up_to(2, 1)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    assert!(matches!(game.input(Viewer::P2), Input::Waiting(_)));
    let result = game.submit_player(Viewer::P2, Command::Concede);
    assert_eq!(result.kind, SubmissionKind::Applied);
    assert_eq!(
        result.input,
        Input::Finished {
            winner: Some(Viewer::P1)
        }
    );
    assert_eq!(game.input(Viewer::P1), result.input);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["s", "d0"])
    );
    let before = exact_authority(&game);
    assert_eq!(
        game.submit_player(Viewer::P1, Command::Concede).kind,
        SubmissionKind::Rejected
    );
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn player_pipeline_failed_draw_ends_by_rules_after_the_answer() {
    let mut game = game(1, 0);
    game.execute_internal(
        &checked(vec![up_to(1, 1)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    let result = answer(
        &mut game,
        Viewer::P1,
        Answer::Stepwise(StepwiseAnswer::Continue),
    );
    assert_eq!(result.kind, SubmissionKind::Applied);
    assert_eq!(
        result.input,
        Input::Finished {
            winner: Some(Viewer::P2)
        }
    );
}

#[test]
fn player_pipeline_prototype_prompt_cannot_appear_as_an_action_window() {
    let mut state = serde_json::to_value(game(1, 3)).unwrap();
    state["state"]["prompt"] = json!({"by":"P1","choices":[],"resume":null});
    let mut game: Game = serde_json::from_value(state).unwrap();
    assert_eq!(game.input(Viewer::P1), Input::EngineFault);
    let before = serde_json::to_value(&game).unwrap();
    game.advance_player_input();
    assert_eq!(game.input(Viewer::P2), Input::EngineFault);
    assert_eq!(serde_json::to_value(&game).unwrap(), before);
}

#[test]
fn player_pipeline_unknown_procedure_cannot_appear_as_a_rule_choice() {
    let mut state = serde_json::to_value(game(1, 3)).unwrap();
    state["state"]["flow"] = json!({"kind":"unknown-procedure"});
    let mut game: Game = serde_json::from_value(state).unwrap();
    assert_eq!(game.input(Viewer::P1), Input::EngineFault);
    game.advance_player_input();
    assert_eq!(game.input(Viewer::P2), Input::EngineFault);
}

// A host supplies its selected coordinates; this adapter never inspects authority state.
#[expect(
    clippy::too_many_lines,
    reason = "Every closed shape has an explicit native answer channel."
)]
pub(super) fn selected_answer(shape: &QuestionShape, selected: &[u64], accept: bool) -> Answer {
    use sve_engine::game::internal::{DamageShare, SearchAnswer, TargetRoleAssignment};
    use sve_engine::game::player::{DecisionShape, DiscardKind};
    let QuestionShape::Decision(shape) = shape else {
        panic!("expected a native decision shape")
    };
    match shape {
        DecisionShape::DiceReroll { .. } | DecisionShape::May => {
            Answer::Integer(Nat(u64::from(accept)))
        }
        DecisionShape::Abilities(options) => {
            Answer::Ability(selected.first().copied().unwrap_or(options[0].handle))
        }
        DecisionShape::ResourceEntryPayment { .. } => Answer::ResourceEntryPayment {
            materials: selected.to_vec(),
            use_ep: accept,
        },
        DecisionShape::WardEntry { .. } => Answer::WardEntry(accept),
        DecisionShape::Quick { .. } => Answer::Quick(selected.first().copied()),
        DecisionShape::OptionalCost | DecisionShape::ResolutionPayment => {
            Answer::OptionalCost(accept)
        }
        DecisionShape::CostMaterials(_)
        | DecisionShape::Discard {
            kind: DiscardKind::Cost,
            ..
        } => Answer::CostMaterials(selected.to_vec()),
        DecisionShape::Discard {
            kind: DiscardKind::Effect,
            ..
        } => Answer::Targets(selected.to_vec()),
        DecisionShape::ExchangeRoutes(routes) => {
            Answer::Integer(selected.first().map_or(routes[0], |value| Nat(*value)))
        }
        DecisionShape::BeforeDraw => Answer::Stepwise(if accept {
            StepwiseAnswer::Continue
        } else {
            StepwiseAnswer::Stop
        }),
        DecisionShape::Pending(options) => {
            Answer::Pending(selected.first().copied().unwrap_or(options[0].handle))
        }
        DecisionShape::TriggerQuota(quota) => Answer::TriggerQuota(
            selected
                .first()
                .copied()
                .unwrap_or(quota.candidates[0].handle),
        ),
        DecisionShape::Integer(bounds) => {
            Answer::Integer(selected.first().map_or(bounds.minimum, |value| Nat(*value)))
        }
        DecisionShape::Order(_) | DecisionShape::DamageReplacements(_) => {
            Answer::Order(selected.to_vec())
        }
        DecisionShape::DriveTrigger { .. } => Answer::DriveTrigger {
            accept,
            target: selected.first().copied(),
        },
        DecisionShape::Search(search) => Answer::Search(search.groups.as_ref().map_or_else(
            || SearchAnswer::Ungrouped(selected.to_vec()),
            |groups| {
                SearchAnswer::Grouped(
                    groups
                        .iter()
                        .map(|group| {
                            selected
                                .iter()
                                .filter(|handle| group.handles.contains(handle))
                                .copied()
                                .collect()
                        })
                        .collect(),
                )
            },
        )),
        DecisionShape::SearchPartition(_) | DecisionShape::MovementCapacity(_) => {
            Answer::SearchPartition(selected.to_vec())
        }
        DecisionShape::Distribution(domain) => {
            let rest = domain
                .minimum
                .0
                .checked_mul(u64::try_from(domain.members.len().saturating_sub(1)).unwrap())
                .unwrap();
            Answer::Distribution(
                domain
                    .members
                    .iter()
                    .enumerate()
                    .map(|(index, member)| DamageShare {
                        handle: member.handle,
                        amount: if index == 0 {
                            Nat(domain.total.0.checked_sub(rest).unwrap())
                        } else {
                            domain.minimum
                        },
                    })
                    .collect(),
            )
        }
        DecisionShape::SearchReplacement => Answer::SearchReplacement(if accept {
            SearchReplacementAnswer::Replacement
        } else {
            SearchReplacementAnswer::Original
        }),
        DecisionShape::Targets(targets) => {
            if targets.roles.is_empty() {
                Answer::Targets(selected.to_vec())
            } else {
                let mut cursor = 0_usize;
                let roles = targets
                    .roles
                    .iter()
                    .map(|role| {
                        let count = match role.cardinality {
                            RoleCardinality::Fixed { cardinality } => {
                                usize::try_from(cardinality.minimum.0).unwrap()
                            }
                            RoleCardinality::IfNonempty { count } => {
                                if selected.is_empty() {
                                    0
                                } else {
                                    usize::try_from(count.0).unwrap()
                                }
                            }
                            RoleCardinality::Remainder {} => {
                                selected.len().checked_sub(cursor).unwrap()
                            }
                        };
                        let end = cursor.checked_add(count).unwrap();
                        let assignment = TargetRoleAssignment {
                            role: role.role,
                            targets: selected[cursor..end].to_vec(),
                        };
                        cursor = end;
                        assignment
                    })
                    .collect();
                Answer::TargetsWithRoles {
                    targets: selected.to_vec(),
                    roles,
                }
            }
        }
        DecisionShape::TokenCapacity(_) => {
            Answer::TokenCapacity(selected.first().copied().unwrap_or(0))
        }
        DecisionShape::Modes { .. } | DecisionShape::ResolutionAlternative { .. } => {
            Answer::Modes(selected.iter().copied().map(ModeId).collect())
        }
    }
}
