use super::*;
use sve_battle_contract::question::{QuestionPurpose, QuestionShape as WireShape, QuestionUse};
use sve_battle_contract::{Answer as WireAnswer, BattleInput, Command as WireCommand, QuestionId};
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

/// BP11-050 puts another 大地の魔片 into play and then adds two stacks. The rules make the
/// player choose which Stack card receives them, and that choice must reach the wire.
#[test]
fn the_recipient_of_a_stack_increase_is_asked_and_answered_over_the_wire() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ウィッチ");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"spell","card":"BP11-050"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"sigil","card":"BP03-T02","state":{"acted":false,"entered_this_turn":false}}
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
    assert_eq!(
        selection.candidates.len(),
        1,
        "the new amulet stacked onto the one in play; a single recipient is still asked"
    );

    let chosen = selection.candidates[0].id.clone();
    let command = projection
        .command(
            Viewer::P1,
            &WireCommand::Answer {
                question_id: id,
                answer: WireAnswer::Select {
                    selected: vec![chosen],
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
    // One stack for the amulet the spell put into play, then the spell's two.
    assert_battle_counter(&game, "BP03-T02", "スタックカウンター", 3);
}
