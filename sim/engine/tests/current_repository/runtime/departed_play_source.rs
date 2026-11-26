use super::*;
use sve_battle_contract::QuestionId;
use sve_engine::game::player::{
    AttackTarget, Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, QuestionShape,
    SelectionQuestion, SubmissionKind, Viewer,
};

/// Answer every stop the same way until a target declaration is asked.
fn advance_to_targets(game: &mut Game) -> SelectionQuestion {
    for _ in 0..8_u8 {
        let (viewer, question) = [Viewer::P1, Viewer::P2]
            .into_iter()
            .find_map(|viewer| match game.input(viewer) {
                Input::Question(question) => Some((viewer, *question)),
                Input::Waiting(_) | Input::Finished { .. } | Input::EngineFault => None,
            })
            .unwrap();
        let command = match question.shape {
            QuestionShape::ActionWindow(actions) => {
                let (intent, target) = actions
                    .intents
                    .iter()
                    .find_map(|intent| match &intent.kind {
                        MainIntentKind::Attack { targets, .. } => targets
                            .iter()
                            .find(|target| matches!(target, AttackTarget::Card(_)))
                            .map(|target| (intent.id.clone(), target.clone())),
                        MainIntentKind::Play { .. }
                        | MainIntentKind::Activate { .. }
                        | MainIntentKind::Evolve { .. }
                        | MainIntentKind::EndPhase => None,
                    })
                    .unwrap();
                Command::MainAction {
                    token: question.token,
                    intent,
                    answer: MainActionAnswer::Attack(target),
                }
            }
            QuestionShape::Decision(DecisionShape::Quick { .. }) => Command::Answer {
                token: question.token,
                answer: Answer::Quick(None),
            },
            QuestionShape::Decision(DecisionShape::Pending(options)) => Command::Answer {
                token: question.token,
                answer: Answer::Pending(options[0].handle),
            },
            QuestionShape::Decision(DecisionShape::Targets(targets)) => return targets,
            other => panic!("unexpected stop: {other:?}"),
        };
        assert_eq!(
            game.submit_player(viewer, command).kind,
            SubmissionKind::Applied,
            "every answer on the way is legal"
        );
    }
    panic!("the Lastword never asked for its target");
}

/// BP16-T04 dies attacking BP01-173 and ceases to exist; its Lastword then declares a target.
/// The play's cost quote must not name the token: no seat can identify it any more.
#[test]
fn a_token_that_left_the_game_asks_its_lastword_target_without_a_source_object() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"token","card":"BP16-T04","state":{"acted":false,"entered_this_turn":false}}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!(cards("deck", "BP01-173", 3));
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"fighter","card":"BP01-173","state":{"acted":true}}]);
    let mut game = game(&initial);
    game.advance_player_input();
    let targets = advance_to_targets(&mut game);

    let projection = game
        .battle_input(
            Viewer::P1,
            &QuestionId::new("lastword".into()),
            battle_references(),
        )
        .unwrap();
    let play = projection.observation().costs.current_play.as_ref();
    assert_eq!(
        play.map(|quote| quote.source.as_ref()),
        Some(None),
        "the quote of the Lastword names no holder"
    );
    game.battle_input(
        Viewer::P2,
        &QuestionId::new("waiting".into()),
        battle_references(),
    )
    .unwrap();

    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("the token's controller declares the target");
    };
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: question.token,
                answer: Answer::Targets(vec![targets.members[0].handle]),
            },
        )
        .kind,
        SubmissionKind::Applied,
        "the declared target is legal"
    );
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P2"]["field_count"], 0_u8, "combat and the Lastword destroy the fighter");
    assert_eq!(view["P1"]["cemetery_count"], 1_u8, "the Lastword mills one card");
}
