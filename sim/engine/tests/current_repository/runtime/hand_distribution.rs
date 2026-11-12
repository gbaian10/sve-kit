#![expect(
    clippy::panic,
    reason = "Unexpected player questions invalidate the printed distribution scenario."
)]
use super::*;
use sve_engine::game::internal::DamageShare;
use sve_engine::game::player::{
    Command, DecisionShape, Input, MainActionAnswer, MainIntentKind, Question, QuestionShape,
    SubmissionKind, Viewer,
};

const CARDS: [(&str, &str, &str); 2] = [
    (
        "BP04-001",
        "c:f8c06510ce375d5cb4aaafbd82c41870",
        "f:7d30e6920e5c5fb7ba19822d5caf9d48",
    ),
    (
        "BP04-002",
        "c:655eb853faef5798ad2ec4ab38bcc9cc",
        "f:486cc34a04c5568f855d7ad38a4edfaa",
    ),
];

#[test]
fn bp04_001_and_002_keep_distinct_sources_and_every_registered_printing() {
    let selected = selection_with_rules();
    let prototypes = serde_json::to_value(&*selected.prototypes).unwrap();
    for (number, card_id, face) in CARDS {
        let author = inputs().2["authors"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["status"] == "accepted" && a["input"]["card_id"] == card_id)
            .unwrap();
        let printings = author["printings"].as_array().unwrap();
        assert!(printings.iter().any(|p| p == number));
        for printing in printings {
            let printing = printing.as_str().unwrap();
            assert_eq!(selected.current[printing].card_id, card_id);
            assert!(prototypes["programs"].get(printing).is_none());
        }
        let card = &selected.current[number];
        assert_eq!(card.source_payload["card_id"], card_id);
        let faces = card.source_payload["faces"].as_array().unwrap();
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0]["face_id"], face);
        assert_eq!(card.program.card().abilities.len(), 1);
        let source = card
            .source_at(&format!(
                "/cards/{number}/faces/{face}/abilities/0/fanfare/do/0/damage/n"
            ))
            .unwrap();
        assert_eq!(source.source.card_id, card_id);
        assert_eq!(source.source.face, face);
        assert_eq!(source.source.field, "effect");
        assert_eq!(source.source.line, 1);
        assert_eq!(source.source.section, None);
    }
}

fn question(game: &Game) -> Question {
    let Input::Question(question) = game.input(Viewer::P1) else {
        panic!("player question")
    };
    *question
}

fn answer(game: &mut Game, answer: Answer) {
    let question = question(game);
    assert_eq!(
        game.submit_player(
            Viewer::P1,
            Command::Answer {
                token: question.token,
                answer,
            }
        )
        .kind,
        SubmissionKind::Applied,
        "the printed fanfare accepts this player answer"
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "Both independent permanent sources share one repository fixture and one printed scenario."
)]
fn bp04_001_and_002_current_sources_distribute_hand_plus_ex_with_overkill() {
    let selected = selection_with_rules();
    for (number, _, _) in CARDS {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":number},
            {"id":"hand-a","card":"BP01-171"},
            {"id":"hand-b","card":"BP01-171"}
        ]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"extra","card":"BP01-171"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy-a","card":"BP01-171","state":{"hp":1_u8,"damage":1_u8}},
            {"id":"enemy-b","card":"BP01-171","state":{"hp":1_u8,"damage":1_u8}}
        ]);
        let mut game = Game::from_selected_effects(
            selected.clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "printed-hand-distribution",
        )
        .unwrap();
        game.advance_player_input();
        let main = question(&game);
        let QuestionShape::ActionWindow(actions) = &main.shape else {
            panic!("main actions")
        };
        let intent = actions
            .intents
            .iter()
            .find(|intent| {
                matches!(
                    &intent.kind, MainIntentKind::Play { card } if card.properties.card == number
                )
            })
            .unwrap();
        assert_eq!(
            game.submit_player(
                Viewer::P1,
                Command::MainAction {
                    token: main.token,
                    intent: intent.id.clone(),
                    answer: MainActionAnswer::None,
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        let pending = question(&game);
        let QuestionShape::Decision(DecisionShape::Pending(options)) = &pending.shape else {
            panic!("printed fanfare")
        };
        assert_eq!(options.len(), 1);
        answer(&mut game, Answer::Pending(options[0].handle));
        let targets = question(&game);
        let QuestionShape::Decision(DecisionShape::Targets(target_domain)) = &targets.shape else {
            panic!("printed targets")
        };
        assert_eq!(target_domain.members.len(), 2);
        assert_eq!(
            target_domain.cardinality,
            Cardinality {
                minimum: Nat(0),
                maximum: None
            }
        );
        answer(
            &mut game,
            Answer::Targets(target_domain.members.iter().map(|m| m.handle).collect()),
        );
        let distribution = question(&game);
        let QuestionShape::Decision(DecisionShape::Distribution(distribution_domain)) =
            &distribution.shape
        else {
            panic!("printed distribution")
        };
        assert_eq!(distribution_domain.total, Nat(3));
        assert_eq!(distribution_domain.minimum, Nat(1));
        let before = game.projection(View::Referee).unwrap();
        for enemy in ["enemy-a", "enemy-b"] {
            assert_eq!(before["objects"][enemy]["hp"], 1_u8);
        }
        answer(
            &mut game,
            Answer::Distribution(
                distribution_domain
                    .members
                    .iter()
                    .zip([2, 1])
                    .map(|(m, amount)| DamageShare {
                        handle: m.handle,
                        amount: Nat(amount),
                    })
                    .collect(),
            ),
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 5_u8);
        assert_eq!(state["P1"]["field"], json!(["source"]));
        assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
        assert_eq!(state["P2"]["field"], json!([]));
        assert_eq!(state["P2"]["cemetery"].as_array().unwrap().len(), 2);
        assert!(matches!(
            question(&game).shape,
            QuestionShape::ActionWindow(_)
        ));
    }
}
