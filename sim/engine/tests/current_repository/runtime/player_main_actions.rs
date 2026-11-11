#![expect(
    clippy::panic,
    reason = "Unexpected real-card input domains fail this serial scenario."
)]

use super::*;
use sve_engine::game::player::{
    AttackTarget, Command, Input, MainActionAnswer, MainIntentKind, Question, QuestionShape,
    SubmissionKind, Viewer,
};

fn question(game: &Game, actor: Viewer) -> Question {
    let Input::Question(question) = game.input(actor) else {
        panic!("player question");
    };
    *question
}

fn quick_pass(game: &mut Game, actor: Viewer) {
    let quick = question(game, actor);
    assert_eq!(
        game.submit_player(
            actor,
            Command::Answer {
                token: quick.token,
                answer: Answer::Quick(None)
            }
        )
        .kind,
        SubmissionKind::Applied,
        "The current Quick Pass must advance the original procedure"
    );
}

#[test]
fn player_main_actions_real_card_attack_and_turn_end_need_only_question_capabilities() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"private-authority-attacker","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"next-draw","card":"BP01-037"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "main-real",
    )
    .unwrap();
    game.advance_player_input();
    let first = question(&game, Viewer::P1);
    let QuestionShape::ActionWindow(actions) = &first.shape else {
        panic!("main actions");
    };
    let attack = actions
        .intents
        .iter()
        .find(|intent| matches!(intent.kind, MainIntentKind::Attack { .. }))
        .unwrap();
    let MainIntentKind::Attack { attacker, targets } = &attack.kind else {
        panic!("attack");
    };
    assert_eq!(attacker.properties.card, "BP01-037");
    let power = attacker
        .properties
        .state
        .as_ref()
        .unwrap()
        .stats
        .unwrap()
        .attack;
    assert!(power > 0);
    assert_eq!(targets, &[AttackTarget::Leader(Viewer::P2)]);
    let command = Command::MainAction {
        token: first.token,
        intent: attack.id.clone(),
        answer: MainActionAnswer::Attack(targets[0].clone()),
    };
    assert_eq!(
        game.submit_player(Viewer::P1, command.clone()).kind,
        SubmissionKind::Applied
    );
    quick_pass(&mut game, Viewer::P2);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        20_i64 - power
    );
    let next = question(&game, Viewer::P1);
    let QuestionShape::ActionWindow(next_actions) = &next.shape else {
        panic!("main after attack");
    };
    assert_eq!(next_actions.intents.len(), 1);
    assert!(matches!(
        next_actions.intents[0].kind,
        MainIntentKind::EndPhase
    ));
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Rejected
    );
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::MainAction {
                token: next.token,
                intent: next_actions.intents[0].id.clone(),
                answer: MainActionAnswer::None
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    quick_pass(&mut game, Viewer::P2);
    assert!(matches!(
        question(&game, Viewer::P2).shape,
        QuestionShape::ActionWindow(_)
    ));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["active"], "P2");
    assert_eq!(state["P2"]["hand"], json!(["next-draw"]));
}
