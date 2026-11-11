#![expect(
    clippy::panic,
    reason = "Serial real-card rule scenarios stop on unexpected public input."
)]

use super::*;
use sve_engine::game::player::observation::ObservedZone;
use sve_engine::game::player::{
    ClosingAnswer, ClosingQuestion, Command, Input, MainActionAnswer, MainIntentKind, ObjectHandle,
    Question, QuestionPurpose, QuestionShape, SubmissionKind, Viewer,
};

fn assert_event_history_available(game: &Game) {
    for viewer in [Viewer::P1, Viewer::P2] {
        let _history = game.events(viewer, None).unwrap();
    }
}

fn board(initial: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "player-closing",
    )
    .unwrap();
    game.advance_player_input();
    assert_event_history_available(&game);
    game
}

fn question(game: &Game, actor: Viewer) -> Question {
    let Input::Question(question) = game.input(actor) else {
        panic!("player question");
    };
    *question
}

fn answer(game: &mut Game, actor: Viewer, answer: Answer) {
    let token = question(game, actor).token;
    assert_eq!(
        game.submit_player(actor, Command::Answer { token, answer })
            .kind,
        SubmissionKind::Applied,
        "The expected typed decision must apply"
    );
    assert_event_history_available(game);
}

fn close(game: &mut Game, actor: Viewer, answer: ClosingAnswer) {
    let token = question(game, actor).token;
    assert_eq!(
        game.submit_player(actor, Command::Closing { token, answer })
            .kind,
        SubmissionKind::Applied,
        "The expected turn-closing answer must apply"
    );
    assert_event_history_available(game);
}

fn end_command(question: &Question) -> Command {
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

fn end(game: &mut Game, actor: Viewer) {
    let command = end_command(&question(game, actor));
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Applied,
        "The active seat must be allowed to begin the end phase"
    );
    assert_event_history_available(game);
}

fn observed_handle(game: &Game, actor: Viewer, zone: ObservedZone, number: &str) -> ObjectHandle {
    game.observe(actor)
        .unwrap()
        .players
        .iter()
        .find(|player| player.player == actor)
        .unwrap()
        .zones
        .iter()
        .find(|entry| entry.kind == zone)
        .unwrap()
        .cards
        .iter()
        .find(|entry| entry.card.properties.card == number)
        .unwrap()
        .card
        .handle
        .clone()
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "The guard, Quick and discard protocol shares stale-token and cross-domain rejection checks."
)]
fn player_closing_guard_quick_and_hand_limit_use_distinct_questions_and_safe_handles() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"guard","card":"CP02-018"},{"id":"acted-guard","card":"CP02-018","state":{"acted":true}}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!(
        (0_u8..8)
            .map(|n| json!({"id":format!("hand-{n}"),"card":"BP01-037"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-037"}]);
    let mut game = board(&initial);
    let original = question(&game, Viewer::P1);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Closing {
                token: original.token,
                answer: ClosingAnswer::GuardAct(vec![]),
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
    assert_eq!(question(&game, Viewer::P1), original);
    end(&mut game, Viewer::P1);
    let guard = question(&game, Viewer::P1);
    assert_eq!(guard.purpose, QuestionPurpose::RuleChoice);
    let QuestionShape::Closing(ClosingQuestion::GuardAct(cards)) = &guard.shape else {
        panic!("end guard");
    };
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].properties.card, "CP02-018");
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: guard.token,
                answer: Answer::WardEntry(true),
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
    assert_eq!(question(&game, Viewer::P1), guard);
    assert!(matches!(game.input(Viewer::P2), Input::Waiting(wait) if wait.actor == Viewer::P1));
    for (actor, payload) in [
        (Viewer::P2, ClosingAnswer::GuardAct(vec![])),
        (
            Viewer::P1,
            ClosingAnswer::Discard(vec![cards[0].handle.clone()]),
        ),
        (
            Viewer::P1,
            ClosingAnswer::GuardAct(vec![cards[0].handle.clone(); 2]),
        ),
    ] {
        assert_eq!(
            game.submit_player(
                actor,
                Command::Closing {
                    token: guard.token,
                    answer: payload
                }
            )
            .kind,
            SubmissionKind::Rejected
        );
        assert_eq!(question(&game, Viewer::P1), guard);
    }
    assert_eq!(
        game.submit_player(Viewer::P1, end_command(&original)).kind,
        SubmissionKind::Rejected
    );
    close(
        &mut game,
        Viewer::P1,
        ClosingAnswer::GuardAct(vec![cards[0].handle.clone()]),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["guard"]["acted"],
        true
    );
    assert_eq!(
        question(&game, Viewer::P2).purpose,
        QuestionPurpose::QuickWindow
    );
    answer(&mut game, Viewer::P2, Answer::Quick(None));
    let discard = question(&game, Viewer::P1);
    let QuestionShape::Closing(ClosingQuestion::Discard { cards: hand, count }) = &discard.shape
    else {
        panic!("hand limit");
    };
    assert_eq!(*count, 1);
    assert_eq!(hand.len(), 8);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Closing {
                token: discard.token,
                answer: ClosingAnswer::Discard(vec![])
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
    assert_eq!(question(&game, Viewer::P1), discard);
    close(
        &mut game,
        Viewer::P1,
        ClosingAnswer::Discard(vec![hand[0].handle.clone()]),
    );
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::ActionWindow(_)
    ));
    let referee = game.projection(View::Referee).unwrap();
    assert_eq!(referee["P1"]["hand"].as_array().unwrap().len(), 7);
    assert_eq!(referee["P2"]["hand"], json!(["draw"]));
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Every unexpected protocol shape, including future additions, must fail this fixed printed chain."
)]
fn player_closing_machinus_recovery_finishes_before_repeating_the_hand_limit_rule() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"tree","card":"BP07-T03"}]);
    let mut hand = vec![json!({"id":"source","card":"BP12-103"})];
    hand.extend((0_u8..7).map(|n| json!({"id":format!("hand-{n}"),"card":"CP03-037"})));
    initial["players"]["P1"]["zones"]["hand"] = json!(hand);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"recovered","card":"BP07-017"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"next-draw","card":"CP03-037"}]);
    let mut game = board(&initial);
    end(&mut game, Viewer::P1);
    answer(&mut game, Viewer::P2, Answer::Quick(None));
    let source = observed_handle(&game, Viewer::P1, ObservedZone::Hand, "BP12-103");
    close(&mut game, Viewer::P1, ClosingAnswer::Discard(vec![source]));
    let mut seen = Vec::new();
    loop {
        let current = question(&game, Viewer::P1);
        let payload = match current.shape {
            QuestionShape::Decision(InputDomain::Pending(options)) => {
                assert_eq!(options.len(), 1);
                assert_eq!(options[0].card, "BP12-103");
                seen.push("pending");
                Answer::Pending(options[0].handle)
            }
            QuestionShape::Decision(InputDomain::Targets(domain)) => {
                assert_eq!(domain.members.len(), 1);
                assert_eq!(domain.members[0].card.as_deref(), Some("BP07-017"));
                seen.push("target");
                Answer::Targets(vec![domain.members[0].handle])
            }
            QuestionShape::Decision(InputDomain::OptionalCost) => {
                seen.push("optional cost");
                Answer::OptionalCost(true)
            }
            QuestionShape::Decision(InputDomain::CostMaterials(domain)) => {
                assert_eq!(domain.members.len(), 1);
                seen.push("tree payment");
                Answer::CostMaterials(vec![domain.members[0].handle])
            }
            QuestionShape::Closing(ClosingQuestion::Discard { count, .. }) => {
                assert_eq!(count, 1);
                break;
            }
            _ => panic!("unexpected Machinus question"),
        };
        assert_eq!(
            game.projection(View::Referee).unwrap()["turn"]["active"],
            "P1"
        );
        answer(&mut game, Viewer::P1, payload);
    }
    assert!(seen.contains(&"pending"));
    assert!(seen.contains(&"target"));
    assert!(seen.contains(&"tree payment"));
    let referee = game.projection(View::Referee).unwrap();
    assert_eq!(referee["P1"]["hand"].as_array().unwrap().len(), 8);
    assert_eq!(referee["objects"]["tree"]["acted"], true);
    let recovered = observed_handle(&game, Viewer::P1, ObservedZone::Hand, "BP07-017");
    close(
        &mut game,
        Viewer::P1,
        ClosingAnswer::Discard(vec![recovered]),
    );
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::ActionWindow(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
}

#[test]
fn player_closing_end_triggers_finish_before_quick_and_start_triggers_follow_the_draw() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"end-source","card":"BP01-157"},{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"start-source","card":"BP04-068"}]);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"keep","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"draw","card":"BP06-048"},{"id":"tail","card":"BP06-048"}]);
    let mut game = board(&initial);
    let power = game.projection(View::Referee).unwrap()["objects"]["ally"]["power"]
        .as_i64()
        .unwrap();
    end(&mut game, Viewer::P1);
    let QuestionShape::Decision(InputDomain::Pending(options)) = question(&game, Viewer::P1).shape
    else {
        panic!("end trigger");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "BP01-157");
    answer(&mut game, Viewer::P1, Answer::Pending(options[0].handle));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["ally"]["power"],
        power + 1
    );
    answer(&mut game, Viewer::P2, Answer::Quick(None));
    let QuestionShape::Decision(InputDomain::Pending(start_options)) =
        question(&game, Viewer::P2).shape
    else {
        panic!("main start trigger");
    };
    assert_eq!(start_options.len(), 1);
    assert_eq!(start_options[0].card, "BP04-068");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["hand"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    answer(
        &mut game,
        Viewer::P2,
        Answer::Pending(start_options[0].handle),
    );
    let QuestionShape::Decision(InputDomain::Targets(domain)) = question(&game, Viewer::P2).shape
    else {
        panic!("start discard");
    };
    let drawn = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP06-048"))
        .unwrap();
    answer(&mut game, Viewer::P2, Answer::Targets(vec![drawn.handle]));
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::ActionWindow(_)
    ));
    let referee = game.projection(View::Referee).unwrap();
    assert_eq!(referee["P2"]["hand"], json!(["keep"]));
    assert_eq!(referee["P2"]["cemetery"], json!(["draw"]));
    assert_eq!(referee["P2"]["deck"], json!(["tail"]));
}

#[test]
fn player_closing_waiting_opponent_can_concede_before_guard_is_answered() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"guard","card":"CP02-018"}]);
    let mut game = board(&initial);
    end(&mut game, Viewer::P1);
    let guard = question(&game, Viewer::P1);
    let terminal = game.submit_player(Viewer::P2, Command::Concede);
    assert_eq!(terminal.kind, SubmissionKind::Applied);
    assert_eq!(
        terminal.input,
        Input::Finished {
            winner: Some(Viewer::P1)
        }
    );
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Closing {
                token: guard.token,
                answer: ClosingAnswer::GuardAct(vec![])
            }
        )
        .kind,
        SubmissionKind::Rejected
    );
}
