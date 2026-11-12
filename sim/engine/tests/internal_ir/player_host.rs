#![expect(
    clippy::panic,
    reason = "Unexpected native questions invalidate these protocol fixtures."
)]
use super::*;
use sve_engine::game::player::{
    Answer, Command, DecisionShape, Input, Question, QuestionShape, Submission, SubmissionKind,
    Viewer,
};
use sve_engine::host::{HostLimits, HostOutcome, PolicyHost};
use sve_engine::policy::PolicyFailure;

const fn limits(max_submissions: u64, max_retries: u64) -> HostLimits {
    HostLimits {
        max_submissions,
        max_retries,
    }
}

fn board() -> Game {
    let mut game = game(1, 3);
    let empty = WellFormedCardProgram::check(CardProgram {
        abilities: Vec::new(),
        card_play_costs: Vec::new(),
    })
    .unwrap();
    for number in ["unit-spell", "unit-follower", "unit-amulet"] {
        game.load_internal_card(number, &empty).unwrap();
    }
    game.advance_player_input();
    game
}

fn prompt(tail: Vec<Located<Node>>) -> Game {
    let mut game = board();
    let mut steps = vec![up_to(1, 1)];
    steps.extend(tail);
    game.execute_internal(&checked(steps), &source("synthetic"), context(1))
        .unwrap();
    game
}

fn question(game: &Game) -> Question {
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("P1 question");
    };
    *question
}

fn bad_submission(host: &mut PolicyHost, game: &mut Game) -> Submission {
    let question = question(game);
    let _command = host
        .prepare(&game.observe(Viewer::P1).unwrap(), &question)
        .unwrap();
    game.submit_player(
        Viewer::P1,
        Command::Answer {
            token: question.token,
            answer: Answer::Modes(Vec::new()),
        },
    )
}

#[test]
fn host_rejects_wrong_payloads_only_up_to_the_configured_retry_limit() {
    for retries in [0_u64, 1] {
        let mut game = prompt(Vec::new());
        let original = question(&game);
        let observation = game.observe(Viewer::P1).unwrap();
        let mut host = PolicyHost::new(limits(10, retries));
        assert_eq!(host.report(), None);
        for attempt in 0..=retries {
            let rejected = bad_submission(&mut host, &mut game);
            assert_eq!(rejected.kind, SubmissionKind::Rejected);
            let outcome = host.record_submission(&rejected);
            if attempt == retries {
                assert_eq!(
                    outcome,
                    Some(HostOutcome::RejectedLimit { actor: Viewer::P1 })
                );
            } else {
                assert_eq!(outcome, None);
            }
            assert_eq!(question(&game), original);
            assert_eq!(game.observe(Viewer::P1).unwrap(), observation);
        }
        let report = host.report().unwrap();
        assert_eq!(report.stats.submissions, retries.checked_add(1).unwrap());
        assert_eq!(report.stats.rejected, report.stats.submissions);
        assert_eq!(report.stats.applied, 0);
        assert_eq!(host.run(&mut game), report);
        assert_eq!(host.prepare(&observation, &original), Err(report.outcome));
        assert_eq!(host.stats(), report.stats);
    }
}

#[test]
fn host_retries_a_real_rejection_then_accepts_the_fresh_correct_answer() {
    let mut game = prompt(Vec::new());
    let mut host = PolicyHost::new(limits(2, 1));
    let rejected = bad_submission(&mut host, &mut game);
    assert_eq!(host.record_submission(&rejected), None);
    let report = host.run(&mut game);
    assert_eq!(report.outcome, HostOutcome::StepLimit);
    assert_eq!(report.stats.submissions, 2);
    assert_eq!(report.stats.rejected, 1);
    assert_eq!(report.stats.applied, 1);
    assert_eq!(report.stats.applied_by_seat, [1, 0]);
    assert!(matches!(game.input(Viewer::P1), Input::Question(_)));
}

#[test]
fn host_new_question_has_its_own_rejection_budget() {
    let mut game = prompt(vec![up_to(2, 1)]);
    let mut host = PolicyHost::new(limits(10, 1));
    let original = question(&game);
    let rejected = bad_submission(&mut host, &mut game);
    assert_eq!(host.record_submission(&rejected), None);
    let command = host
        .prepare(&game.observe(Viewer::P1).unwrap(), &original)
        .unwrap();
    let applied = game.submit_player(Viewer::P1, command);
    assert_eq!(applied.kind, SubmissionKind::Applied);
    assert_eq!(host.record_submission(&applied), None);
    assert_ne!(question(&game).token, original.token);
    let rejected = bad_submission(&mut host, &mut game);
    assert_eq!(host.record_submission(&rejected), None);
    let rejected = bad_submission(&mut host, &mut game);
    assert_eq!(
        host.record_submission(&rejected),
        Some(HostOutcome::RejectedLimit { actor: Viewer::P1 })
    );
    assert_eq!(host.stats().submissions, 4);
    assert_eq!(host.stats().rejected, 3);
}

#[test]
fn host_prepare_enforces_command_limits_and_pairing_without_sending_commands() {
    let mut game = prompt(Vec::new());
    let question = question(&game);
    let observation = game.observe(Viewer::P1).unwrap();
    let mut limited = PolicyHost::new(limits(0, 1));
    assert_eq!(
        limited.prepare(&observation, &question),
        Err(HostOutcome::StepLimit)
    );
    assert_eq!(limited.stats().submissions, 0);
    let mut paired = PolicyHost::new(limits(10, 1));
    let _command = paired.prepare(&observation, &question).unwrap();
    assert_eq!(
        paired.prepare(&observation, &question),
        Err(HostOutcome::ProtocolError)
    );
    assert_eq!(paired.stats().submissions, 0);
    let rejected = game.submit_player(
        Viewer::P1,
        Command::Answer {
            token: question.token,
            answer: Answer::Modes(Vec::new()),
        },
    );
    let mut unmatched = PolicyHost::new(limits(10, 1));
    assert_eq!(
        unmatched.record_submission(&rejected),
        Some(HostOutcome::ProtocolError)
    );
    assert_eq!(unmatched.stats().submissions, 0);
    assert_eq!(
        unmatched.record_submission(&rejected),
        Some(HostOutcome::ProtocolError)
    );
}

#[test]
fn host_policy_failure_is_distinct_from_wrong_viewer_protocol_and_engine_fault() {
    let game = prompt(Vec::new());
    let mut question = question(&game);
    let observation = game.observe(Viewer::P1).unwrap();
    let mut wrong_viewer = PolicyHost::new(limits(10, 1));
    assert_eq!(
        wrong_viewer.prepare(&game.observe(Viewer::P2).unwrap(), &question),
        Err(HostOutcome::ProtocolError)
    );
    question.shape = QuestionShape::Decision(DecisionShape::Abilities(Vec::new()));
    let mut host = PolicyHost::new(limits(10, 1));
    assert_eq!(
        host.prepare(&observation, &question),
        Err(HostOutcome::PolicyFailure {
            actor: Viewer::P1,
            failure: PolicyFailure::NoSolution,
        })
    );
    assert_eq!(host.stats().submissions, 0);
    assert!(matches!(game.input(Viewer::P1), Input::Question(_)));
}

#[test]
fn host_actual_execution_fault_wins_over_the_command_limit_without_a_winner() {
    let mut game = prompt(vec![draw(2, 10_001)]);
    let mut host = PolicyHost::new(limits(1, 1));
    let report = host.run(&mut game);
    assert_eq!(report.outcome, HostOutcome::EngineFault);
    assert_eq!(report.stats.submissions, 1);
    assert_eq!(report.stats.engine_faults, 1);
    assert_eq!(report.stats.applied, 0);
    for viewer in [Viewer::P1, Viewer::P2] {
        assert_eq!(game.input(viewer), Input::EngineFault);
    }
    let mut already_faulted = PolicyHost::new(limits(0, 0));
    assert_eq!(
        already_faulted.run(&mut game).outcome,
        HostOutcome::EngineFault
    );
    assert_eq!(already_faulted.stats().submissions, 0);
}

#[test]
fn host_rules_finish_on_the_last_permitted_command_is_not_step_limit() {
    let mut game = prompt(vec![damage(
        2,
        RecipientSet::Leaders {
            players: players(PlayerRelation::Opponents),
        },
        20,
    )]);
    let mut host = PolicyHost::new(limits(1, 0));
    let report = host.run(&mut game);
    assert_eq!(
        report.outcome,
        HostOutcome::Finished {
            winner: Some(Viewer::P1)
        }
    );
    assert_eq!(report.stats.submissions, 1);
    assert_eq!(report.stats.applied, 1);
    assert_eq!(host.run(&mut game), report);
}

#[test]
fn host_existing_win_or_draw_has_precedence_over_zero_command_quota() {
    let mut conceded = board();
    let outcome = conceded.submit_player(Viewer::P1, Command::Concede);
    assert_eq!(
        outcome.input,
        Input::Finished {
            winner: Some(Viewer::P2)
        }
    );
    let mut host = PolicyHost::new(limits(0, 0));
    assert_eq!(
        host.run(&mut conceded).outcome,
        HostOutcome::Finished {
            winner: Some(Viewer::P2)
        }
    );
    assert_eq!(host.stats().submissions, 0);
    let mut tied = board();
    tied.execute_internal(
        &checked(vec![damage(
            1,
            RecipientSet::Leaders {
                players: players(PlayerRelation::All),
            },
            20,
        )]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    tied.advance_player_input();
    let mut draw_host = PolicyHost::new(limits(0, 0));
    assert_eq!(
        draw_host.run(&mut tied).outcome,
        HostOutcome::Finished { winner: None }
    );
    assert_eq!(draw_host.stats().submissions, 0);
}
