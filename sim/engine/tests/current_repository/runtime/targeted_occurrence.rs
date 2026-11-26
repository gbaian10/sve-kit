use super::*;
use sve_battle_contract::question::{ChoiceValue, QuestionShape as WireShape, TriggerEvent};
use sve_battle_contract::{BattleInput, Nat as WireNat, QuestionId};
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, PendingChoice, QuestionShape,
    SubmissionKind, Viewer,
};

/// Activate the amulet on the lizard and stop at the lizard's pending occurrence.
fn advance_to_pending(game: &mut Game) -> Vec<PendingChoice> {
    for _ in 0..8_u8 {
        let Input::Question(question) = game.input(Viewer::P1) else {
            panic!("the turn player is asked at every stop of its own activation");
        };
        let answer = match question.shape {
            QuestionShape::ActionWindow(actions) => {
                let intent = actions
                    .intents
                    .iter()
                    .find(|intent| matches!(intent.kind, MainIntentKind::Activate { .. }))
                    .unwrap();
                Command::MainAction {
                    token: question.token,
                    intent: intent.id.clone(),
                    answer: MainActionAnswer::None,
                }
            }
            QuestionShape::Decision(DecisionShape::Targets(targets)) => Command::Answer {
                token: question.token,
                answer: Answer::Targets(vec![targets.members[0].handle]),
            },
            QuestionShape::Decision(DecisionShape::Pending(options)) => return options,
            other => panic!("unexpected stop: {other:?}"),
        };
        assert_eq!(
            game.submit_player(Viewer::P1, answer).kind,
            SubmissionKind::Applied,
            "every answer on the way is legal"
        );
    }
    panic!("the lizard's ability never became pending");
}

/// BP02-T07 chooses ETD01-011, whose "when chosen by an ability" trigger then waits to be
/// resolved. The wire must be able to say which completed event made that occurrence pending.
#[test]
fn an_occurrence_of_being_chosen_is_offered_over_the_wire() {
    let mut initial = draw_position();
    initial["players"]["P1"]["leader"]["class"] = json!("ドラゴン");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"lizard","card":"ETD01-011","state":{"acted":false,"entered_this_turn":false}},
        {"id":"weapon","card":"BP02-T07","state":{"acted":false,"entered_this_turn":false}}
    ]);
    let mut game = game(&initial);
    game.advance_player_input();
    let pending = advance_to_pending(&mut game);
    assert_eq!(pending.len(), 1, "only the lizard was chosen");

    let projection = game
        .battle_input(
            Viewer::P1,
            &QuestionId::new("chosen".into()),
            battle_references(),
        )
        .unwrap();
    let BattleInput::Question { question } = projection.input() else {
        panic!("the controller orders its pending occurrence");
    };
    let WireShape::Choose { options, .. } = &question.shape else {
        panic!("a pending order is a choice");
    };
    let [option] = options.as_slice() else {
        panic!("one occurrence is pending");
    };
    let ChoiceValue::Occurrence {
        event, position, ..
    } = &option.value
    else {
        panic!("the option is an occurrence");
    };
    assert_eq!(*event, TriggerEvent::Targeted);
    assert_eq!(
        *position,
        Some(WireNat::ZERO),
        "the lizard was in the first field slot when it was chosen"
    );

    let Input::Question(asked) = game.input(Viewer::P1) else {
        panic!("the occurrence is still pending");
    };
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: asked.token,
                answer: Answer::Pending(pending[0].handle),
            },
        )
        .kind,
        SubmissionKind::Applied,
        "the pending occurrence resolves"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["objects"]["lizard"]["power"], 4_u8, "attack +1");
    assert_eq!(view["P2"]["leader"]["life"], 19_u8, "one damage to the opposing leader");
}
