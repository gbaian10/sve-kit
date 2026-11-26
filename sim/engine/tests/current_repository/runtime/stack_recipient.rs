use super::*;
use sve_battle_contract::question::{
    Candidate, QuestionPurpose, QuestionShape as WireShape, QuestionUse,
};
use sve_battle_contract::{
    Answer as WireAnswer, BattleInput, Command as WireCommand, ObjectHandle, QuestionId, Subject,
};
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, QuestionShape, SubmissionKind,
    Viewer,
};

/// Play the spell and answer the stops on the way until objects are asked for.
fn advance_to_recipient(game: &mut Game) {
    for _ in 0..8_u8 {
        let Input::Question(question) = game.input(Viewer::P1) else {
            panic!("the turn player is asked at every stop of its own spell");
        };
        let answer = match question.shape {
            QuestionShape::ActionWindow(actions) => {
                let intent = actions
                    .intents
                    .iter()
                    .find(|intent| matches!(intent.kind, MainIntentKind::Play { .. }))
                    .unwrap();
                Command::MainAction {
                    token: question.token,
                    intent: intent.id.clone(),
                    answer: MainActionAnswer::None,
                }
            }
            QuestionShape::Decision(DecisionShape::Pending(options)) => Command::Answer {
                token: question.token,
                answer: Answer::Pending(options[0].handle),
            },
            QuestionShape::Decision(DecisionShape::Targets(_)) => return,
            other => panic!("unexpected stop: {other:?}"),
        };
        assert_eq!(
            game.submit_player(Viewer::P1, answer).kind,
            SubmissionKind::Applied,
            "every answer on the way is legal"
        );
    }
    panic!("the Stack increase never asked for its recipient");
}

/// BP11-050 puts a second 大地の魔片 into play and then adds two stacks. The rules make the
/// player choose which of its Stack cards receives them, and that choice must reach the wire.
#[test]
fn the_recipient_of_a_stack_increase_is_asked_and_answered_over_the_wire() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ウィッチ");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP11-050"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"sigil","card":"BP03-T02","state":{"counters":{"stack_counter":1_u8}}}
    ]);
    let mut game = game(&initial);
    game.advance_player_input();
    advance_to_recipient(&mut game);

    let id = QuestionId::new("stack".into());
    let projection = game
        .battle_input(Viewer::P1, &id, battle_references())
        .unwrap();
    let BattleInput::Question { question } = projection.input() else {
        panic!("the controller chooses the recipient");
    };
    assert_eq!(question.usage, QuestionUse::StackRecipient);
    assert_eq!(question.purpose, QuestionPurpose::RuleChoice);
    let WireShape::Select { selection } = &question.shape else {
        panic!("the recipient is a selection");
    };
    let [first, second] = selection.candidates.as_slice() else {
        panic!("the amulet in play and the one the spell just put there are both candidates");
    };
    let handle = |candidate: &Candidate| match &candidate.subject {
        Some(Subject::Card {
            object: Some(object),
            ..
        }) => object.clone(),
        other => panic!("a Stack card on the field is identifiable: {other:?}"),
    };
    // The second candidate: an answer mapped to the first one regardless would be caught.
    let (chosen, other) = (handle(second), handle(first));

    let chosen_id = second.id.clone();
    let command = projection
        .command(
            Viewer::P1,
            &WireCommand::Answer {
                question_id: id,
                answer: WireAnswer::Select {
                    selected: vec![chosen_id],
                    groups: None,
                    roles: vec![],
                },
            },
        )
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Applied,
        "the listed candidate is a legal recipient"
    );
    assert!(
        matches!(
            game.input(Viewer::P1),
            Input::Question(next) if matches!(next.shape, QuestionShape::ActionWindow(_))
        ),
        "the spell finished and the main phase goes on"
    );
    let objects = game
        .battle_objects(Viewer::P1, battle_references())
        .unwrap();
    let stacks = |object: &ObjectHandle| {
        objects[object]
            .state
            .as_ref()
            .unwrap()
            .counters
            .iter()
            .find(|counter| counter.name == "スタックカウンター")
            .map(|counter| counter.amount.get())
    };
    // Each amulet holds one stack of its own; both of the spell's go to the chosen one.
    assert_eq!(stacks(&chosen), Some(3), "the chosen amulet receives the increase");
    assert_eq!(stacks(&other), Some(1), "the other amulet is unchanged");
}
