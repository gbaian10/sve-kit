use super::declarations::declaration_actor;
use super::hand_exchange_declarations::{apply, seat};
use super::recipient_declarations::{distribution_program, recipient_program, shares, start};
use super::target_declarations::target_program;
use super::*;

fn stage(game: &Game, expected: Option<QuestionStage>) {
    let before = exact_authority(game);
    assert_eq!(
        game.internal_question_stage(),
        expected,
        "the current wait owns its producer-recorded phase"
    );
    for viewer in [Viewer::P1, Viewer::P2] {
        game.observe(viewer).unwrap();
    }
    assert_eq!(
        game.internal_question_stage(),
        expected,
        "observation preserves the phase"
    );
    assert!(
        exact_authority(game) == before,
        "reading phase and observing either seat is pure"
    );
}

#[test]
fn formal_object_target_wait_has_its_own_producer_stage() {
    let program = target_program();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        let q = question(&game, actor);
        assert_eq!(q.purpose, QuestionPurpose::PlayParameter);
        assert_eq!(game.internal_play_phase(), Some(PlayPhase::Declare));
        stage(&game, Some(QuestionStage::Target));
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        let before = exact_authority(&game);
        assert_eq!(
            submit(&mut game, other, &q, Answer::Targets(vec![0])),
            SubmissionKind::Rejected
        );
        for answer in [
            Answer::Integer(Nat(0)),
            Answer::Targets(vec![99]),
            Answer::Targets(vec![0, 0]),
        ] {
            assert_eq!(
                submit(&mut game, actor, &q, answer),
                SubmissionKind::Rejected
            );
        }
        assert!(
            exact_authority(&game) == before,
            "rejected formal targets preserve the phase and authority"
        );
        stage(&game, Some(QuestionStage::Target));
        closed_wire(&game, actor);
        apply(&mut game, actor, Answer::Targets(vec![0]));
        stage(&game, Some(QuestionStage::Resolve));
        finish_may(&mut game);
        stage(&game, None);
    }
}

#[test]
fn formal_and_resolution_recipient_questions_record_their_actual_producers() {
    let program = recipient_program(true);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        assert_eq!(
            question(&game, actor).purpose,
            QuestionPurpose::PlayParameter
        );
        stage(&game, Some(QuestionStage::Target));
        apply(&mut game, actor, Answer::Targets(vec![0]));
        let resolving = question(&game, actor);
        assert_eq!(resolving.purpose, QuestionPurpose::ResolutionChoice);
        assert!(matches!(
            resolving.shape,
            QuestionShape::Decision(player::DecisionShape::Targets(_))
        ));
        stage(&game, Some(QuestionStage::Resolve));
        apply(&mut game, actor, Answer::Targets(vec![0]));
        stage(&game, Some(QuestionStage::Resolve));
        finish_may(&mut game);
        stage(&game, None);
    }
}

#[test]
fn an_object_choice_during_resolution_records_resolve_without_an_earlier_may() {
    let program = program(
        "activate",
        json!({"cost":{"pp":1},"do":[
            {"choose":{"bind":"picked","from":{"who":"opp","zone":"field","kind":"follower"},"n":1}},
            {"optional":{"do":[{"life":{"who":"self","delta":1}}]}}
        ]}),
    );
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        let q = question(&game, actor);
        assert_eq!(q.purpose, QuestionPurpose::ResolutionChoice);
        assert!(matches!(
            q.shape,
            QuestionShape::Decision(player::DecisionShape::Targets(_))
        ));
        stage(&game, Some(QuestionStage::Resolve));
        apply(&mut game, actor, Answer::Targets(vec![0]));
        stage(&game, Some(QuestionStage::Resolve));
        finish_may(&mut game);
        stage(&game, None);
    }
}

#[test]
fn formal_allocation_records_allocate_after_a_target_retry_and_preserves_it_on_rejection() {
    let program = distribution_program(false);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 4);
        stage(&game, Some(QuestionStage::Target));
        apply(&mut game, actor, Answer::Targets(vec![0, 1, 2, 3]));
        stage(&game, Some(QuestionStage::Target));
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat(actor)]["pp"]["current"],
            3
        );
        apply(&mut game, actor, Answer::Targets(vec![0, 1]));
        let q = question(&game, actor);
        assert_eq!(q.purpose, QuestionPurpose::PlayParameter);
        stage(&game, Some(QuestionStage::Allocate));
        let before = exact_authority(&game);
        for answer in [
            Answer::Targets(vec![0]),
            allocation::answer(&[(0, 0), (1, 0)]),
        ] {
            assert_eq!(
                submit(&mut game, actor, &q, answer),
                SubmissionKind::Rejected
            );
        }
        assert!(
            exact_authority(&game) == before,
            "rejected shares preserve the allocation phase"
        );
        stage(&game, Some(QuestionStage::Allocate));
        closed_wire(&game, actor);
        apply(&mut game, actor, shares());
        stage(&game, Some(QuestionStage::Resolve));
        finish_may(&mut game);
        stage(&game, None);
    }
}

fn distribution_then_number(empty: bool, foreign: bool) -> WellFormedProgram {
    let recipients = if empty {
        RecipientSet::Explicit { recipients: vec![] }
    } else {
        allocation::leaders()
    };
    let actor = declaration_actor(foreign);
    let mut distribution = allocation::distribution(1001, recipients, Expr::Literal(Nat(3)), 0);
    let changed = if let Node::Decision { decision } = &mut distribution.node {
        decision.actor = actor.clone();
        true
    } else {
        false
    };
    assert!(
        changed,
        "the fixture starts with an actual distribution decision"
    );
    checked(vec![
        distribution,
        instruction(
            1002,
            Action::Declare {
                actor,
                declaration: Declaration::NonnegativeInteger {
                    binding: Binding::new(1002, ScopeId(1)),
                },
            },
        ),
    ])
}

#[test]
fn resolution_allocation_and_empty_skip_do_not_leave_a_phase_on_an_unrecorded_question() {
    for controller in [Viewer::P1, Viewer::P2] {
        for foreign in [false, true] {
            let actor = if foreign {
                if controller == Viewer::P1 {
                    Viewer::P2
                } else {
                    Viewer::P1
                }
            } else {
                controller
            };
            for empty in [false, true] {
                let program = distribution_then_number(empty, foreign);
                let mut game = game(controller.player().seat, 0);
                let progress = game
                    .execute_internal(
                        &program,
                        &source("synthetic"),
                        context(controller.player().seat),
                    )
                    .unwrap();
                assert!(!progress.complete);
                assert_eq!(game.internal_play_phase(), None);
                if !empty {
                    assert_eq!(
                        question(&game, actor).purpose,
                        QuestionPurpose::ResolutionChoice
                    );
                    stage(&game, Some(QuestionStage::Allocate));
                    apply(&mut game, actor, shares());
                }
                let number = question(&game, actor);
                assert!(matches!(
                    number.shape,
                    QuestionShape::Decision(player::DecisionShape::Integer { .. })
                ));
                stage(&game, None);
                apply(&mut game, actor, Answer::Integer(Nat(0)));
                stage(&game, None);
            }
        }
    }
}

#[test]
fn a_body_fault_retires_both_formal_target_and_resolution_phase() {
    let program = lifecycle::with_overflow(&target_program());
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = start(&program, actor, 2);
        stage(&game, Some(QuestionStage::Target));
        apply(&mut game, actor, Answer::Targets(vec![0]));
        stage(&game, Some(QuestionStage::Resolve));
        let q = question(&game, actor);
        assert_eq!(
            submit(&mut game, actor, &q, Answer::Integer(Nat(1))),
            SubmissionKind::EngineFault
        );
        stage(&game, None);
        assert_eq!(game.internal_play_phase(), None);
        assert_eq!(
            game.projection(View::Referee).unwrap()[seat(actor)]["pp"]["current"],
            3
        );
    }
}
