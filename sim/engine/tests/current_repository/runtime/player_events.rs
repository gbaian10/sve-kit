#![expect(
    clippy::panic,
    reason = "The real-card protocol must expose the expected typed question."
)]

use super::*;
use sve_engine::game::player::events::{ActionKind, EventKind, MovementMethod, PlayerEvent};
use sve_engine::game::player::{
    Command, Input, MainActionAnswer, MainIntentKind, Question, QuestionShape, SubmissionKind,
    Viewer,
};

fn question(game: &Game) -> Question {
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("player question");
    };
    *question
}

fn declare_spell(game: &mut Game) {
    let main = question(game);
    let QuestionShape::ActionWindow(actions) = &main.shape else {
        panic!("main declarations");
    };
    let play = actions.intents.iter().find(|intent| matches!(&intent.kind, MainIntentKind::Play { card } if card.properties.card == "BP04-066")).unwrap();
    let submission = game.submit_player(
        Viewer::P1,
        Command::MainAction {
            token: main.token,
            intent: play.id.clone(),
            answer: MainActionAnswer::None,
        },
    );
    assert_eq!(
        submission.kind,
        SubmissionKind::Applied,
        "The public intent starts the real card declaration"
    );
    assert!(
        matches!(submission.input, Input::Question(_)),
        "The accepted declaration continues with its existing typed question"
    );
}

fn history(game: &Game, viewer: Viewer) -> Vec<PlayerEvent> {
    game.events(viewer, None).unwrap().cloned().collect()
}

fn assert_completed_spell_history(events: &[PlayerEvent]) {
    for expected in [ActionKind::Play, ActionKind::Resolve] {
        assert_eq!(events.iter().filter(|event| matches!(&event.kind, EventKind::Action(action) if action.kind == expected)).count(), 1,
            "Step and Fact describe the same committed producer");
        assert!(events.iter().filter(|event| matches!(&event.kind, EventKind::Action(action) if action.kind == expected))
            .all(|event| matches!(&event.kind, EventKind::Action(action) if action.player == Some(Viewer::P1))),
            "The typed proper-play controller is retained when its Step has no actor field");
    }
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(
                &event.kind,
                EventKind::Movement {
                    method: MovementMethod::Effect(MoveMethod::Banish),
                    ..
                }
            ))
            .count(),
        2,
        "The accepted surcharge banishes both followers with one physical result each"
    );
    for event in events {
        if let EventKind::Action(action) = &event.kind
            && matches!(action.kind, ActionKind::Play | ActionKind::Resolve)
        {
            assert_eq!(
                action.player,
                Some(Viewer::P1),
                "The typed proper-play controller survives a Step without an actor field"
            );
        }
    }
}

#[test]
fn player_events_real_multi_question_payment_commits_once_or_reverts_without_history() {
    for pp in [9_u8, 10] {
        let mut initial = draw_position();
        initial["players"]["P1"]["pp"] = json!({"current":pp,"max":10_u8});
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP04-066"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy1","card":"BP01-037"},{"id":"enemy2","card":"BP01-037"}
        ]);
        let mut game = game(&initial);
        game.advance_player_input();
        let pristine = game.clone();
        declare_spell(&mut game);
        let optional = question(&game);
        assert!(matches!(
            optional.shape,
            QuestionShape::Decision(InputDomain::OptionalCost)
        ));
        assert_eq!(
            game.submit_player(
                Viewer::P1,
                Command::Answer {
                    token: optional.token,
                    answer: Answer::OptionalCost(true)
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        let target = question(&game);
        assert!(matches!(
            target.shape,
            QuestionShape::Decision(InputDomain::Targets(_))
        ));
        for viewer in [Viewer::P1, Viewer::P2] {
            assert_eq!(history(&game, viewer), vec![]);
        }
        assert_eq!(
            game.submit_player(
                Viewer::P1,
                Command::Answer {
                    token: target.token,
                    answer: Answer::Targets(vec![])
                }
            )
            .kind,
            SubmissionKind::Rejected
        );
        let submitted = game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: target.token,
                answer: Answer::Targets(vec![0]),
            },
        );
        if pp == 9 {
            assert_eq!(submitted.kind, SubmissionKind::Reverted);
            assert_eq!(
                game.projection(View::Referee).unwrap(),
                pristine.projection(View::Referee).unwrap()
            );
            for viewer in [Viewer::P1, Viewer::P2] {
                assert_eq!(history(&game, viewer), vec![]);
            }
            let mut control = pristine;
            assert_eq!(
                control.submit_player(Viewer::P2, Command::Concede).kind,
                SubmissionKind::Applied
            );
            assert_eq!(
                game.submit_player(Viewer::P2, Command::Concede).kind,
                SubmissionKind::Applied
            );
            assert_eq!(
                history(&game, Viewer::P1),
                history(&control, Viewer::P1),
                "Rollback consumes no public occurrence or cause coordinate"
            );
        } else {
            assert_eq!(submitted.kind, SubmissionKind::Applied);
            for viewer in [Viewer::P1, Viewer::P2] {
                let events = history(&game, viewer);
                assert_completed_spell_history(&events);
                let cursor = events.last().unwrap().id.clone();
                game.advance_player_input();
                assert_eq!(game.events(viewer, Some(&cursor)).unwrap().count(), 0);
            }
        }
    }
}
