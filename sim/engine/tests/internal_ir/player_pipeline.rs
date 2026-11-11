#![expect(
    clippy::panic,
    reason = "Unexpected public input shapes fail the fixture."
)]

use super::*;
use sve_engine::game::player::{
    Command, Input, Question, QuestionShape, Submission, SubmissionKind, Viewer,
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
        QuestionShape::ActionWindow
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["s", "d0"])
    );
}

#[test]
fn player_pipeline_fault_rolls_back_the_play_without_a_rules_defeat() {
    let mut game = game(1, 3);
    let origin = game.clone();
    game.execute_internal(
        &checked(vec![draw(1, 1), up_to(2, 1), draw(3, 10_001)]),
        &source("synthetic"),
        context(1),
    )
    .unwrap();
    let failed = answer(
        &mut game,
        Viewer::P1,
        Answer::Stepwise(StepwiseAnswer::Stop),
    );
    assert_eq!(failed.kind, SubmissionKind::EngineFault);
    assert_eq!(failed.input, Input::EngineFault);
    assert_eq!(game.input(Viewer::P2), Input::EngineFault);
    let expected = serde_json::to_value(&origin).unwrap();
    let mut restored = serde_json::to_value(&game).unwrap();
    assert!(
        restored["state"]["ir_epoch"].as_u64().unwrap()
            >= expected["state"]["ir_epoch"].as_u64().unwrap()
    );
    restored["state"]["ir_epoch"] = expected["state"]["ir_epoch"].clone();
    assert_eq!(
        restored, expected,
        "all authority and RNG data must roll back"
    );
    assert!(!format!("{failed:?}").contains("draw request"));
    game.advance_player_input();
    assert_eq!(game.input(Viewer::P1), Input::EngineFault);
    assert_ne!(
        game.projection(View::Referee).unwrap()["game"]["ended"],
        true
    );
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
