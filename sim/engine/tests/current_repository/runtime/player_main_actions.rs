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

#[test]
fn basic_policy_real_attack_uses_observation_and_question_without_authority_reads() {
    use sve_engine::policy::{BasicPolicy, Choice};

    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"authority-attacker","card":"BP01-037"}]);
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "policy-real",
    )
    .unwrap();
    game.advance_player_input();
    let mut policy = BasicPolicy::new(Viewer::P1);
    let public = question(&game, Viewer::P1);
    let observation = game.observe(Viewer::P1).unwrap();
    let choice = policy.choose(&observation, &public.shape).unwrap();
    let Choice::MainAction { intent, answer } = &choice else {
        panic!("main choice");
    };
    let MainIntentKind::Attack { attacker, .. } = &intent.kind else {
        panic!("attack choice");
    };
    let power = attacker
        .properties
        .state
        .as_ref()
        .unwrap()
        .stats
        .unwrap()
        .attack;
    assert!(power > 0);
    assert_eq!(
        *answer,
        MainActionAnswer::Attack(AttackTarget::Leader(Viewer::P2))
    );
    let submission = game.submit_player(
        Viewer::P1,
        Command::MainAction {
            token: public.token,
            intent: intent.id.clone(),
            answer: answer.clone(),
        },
    );
    assert_eq!(submission.kind, SubmissionKind::Applied);
    policy.record_submission(&choice, submission.kind);
    let quick_question = question(&game, Viewer::P2);
    let mut opponent = BasicPolicy::new(Viewer::P2);
    let quick_choice = opponent
        .choose(&game.observe(Viewer::P2).unwrap(), &quick_question.shape)
        .unwrap();
    let Choice::Decision(quick_answer) = &quick_choice else {
        panic!("Quick answer");
    };
    assert_eq!(*quick_answer, Answer::Quick(None));
    let quick_submission = game.submit_player(
        Viewer::P2,
        Command::Answer {
            token: quick_question.token,
            answer: quick_answer.clone(),
        },
    );
    assert_eq!(quick_submission.kind, SubmissionKind::Applied);
    opponent.record_submission(&quick_choice, quick_submission.kind);
    let after_attack = question(&game, Viewer::P1);
    let end_choice = policy
        .choose(&game.observe(Viewer::P1).unwrap(), &after_attack.shape)
        .unwrap();
    assert!(
        matches!(end_choice, Choice::MainAction { intent: end_intent, answer: MainActionAnswer::None }
        if matches!(end_intent.kind, MainIntentKind::EndPhase))
    );
    assert_eq!(
        game.observe(Viewer::P1).unwrap().players[1].resources.life,
        20_i64.checked_sub(power).unwrap()
    );
}
