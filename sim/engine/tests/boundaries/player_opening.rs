#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "Literal opening fixtures fail immediately on malformed state or questions."
)]

use super::*;
use sve_engine::game::player::{
    Command, Input, OpeningAnswer, OpeningQuestion, Question, QuestionShape, SubmissionKind, Viewer,
};

fn question(game: &Game, actor: Viewer) -> Question {
    let Input::Question(question) = game.input(actor) else {
        panic!("opening question");
    };
    *question
}

fn initial(random: &Value) -> Game {
    let mut players = json!({});
    for seat in ["P1", "P2"] {
        let mut deck = vec![
            json!({"id":format!("{seat}-start"),"card":"unit-follower"}),
            json!({"id":format!("{seat}-leftover"),"card":"unit-follower"}),
        ];
        deck.extend(
            (0_u8..8).map(|index| json!({"id":format!("{seat}-{index}"),"card":"unit-spell"})),
        );
        players[seat] = json!({"construction":"title","title_code":"synthetic_start",
            "leader":{"class":"ニュートラル"},"deck_list":deck,"evolve_deck_list":[]});
    }
    Game::new(
        opening_catalog(),
        &json!({"pregame":true,"players":players}),
        &Value::Null,
        random,
        "player-amulet",
    )
    .unwrap()
}

#[test]
fn player_opening_fault_rolls_back_the_accepted_answer_and_stays_terminal_for_input() {
    let random = json!({"shuffles":[{"player":"P2","zone":"deck","result":[]}]});
    let mut game = initial(&random);
    for (actor, expected) in [
        (Viewer::P1, SubmissionKind::Applied),
        (Viewer::P2, SubmissionKind::EngineFault),
    ] {
        let current = question(&game, actor);
        let QuestionShape::Opening(OpeningQuestion::StartAmulet(cards)) = current.shape else {
            panic!("amulet");
        };
        let before = game.observe(actor).unwrap();
        assert_eq!(
            game.submit_player(
                actor,
                Command::Opening {
                    token: current.token,
                    answer: OpeningAnswer::StartAmulet(cards[0].handle.clone())
                }
            )
            .kind,
            expected
        );
        if expected == SubmissionKind::EngineFault {
            assert_eq!(game.observe(actor).unwrap(), before);
            for viewer in [Viewer::P1, Viewer::P2] {
                assert_eq!(game.input(viewer), Input::EngineFault);
            }
            assert_eq!(
                game.submit_player(
                    actor,
                    Command::Opening {
                        token: current.token,
                        answer: OpeningAnswer::StartAmulet(cards[0].handle.clone())
                    }
                )
                .kind,
                SubmissionKind::EngineFault
            );
        }
    }
}
