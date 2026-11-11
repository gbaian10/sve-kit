#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "Literal opening fixtures fail immediately on malformed state or questions."
)]

use super::*;
use sve_engine::game::player::observation::ObservedZone;
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
#[expect(
    clippy::too_many_lines,
    reason = "Both seats share one sequential opening and privacy scenario."
)]
fn player_opening_start_amulets_are_serial_and_only_revealed_after_both_mulligans() {
    let mut game = initial(&json!({"first_chooser":"P2"}));
    let mut old = Vec::new();
    for actor in [Viewer::P1, Viewer::P2] {
        let question = question(&game, actor);
        let QuestionShape::Opening(OpeningQuestion::StartAmulet(cards)) = question.shape else {
            panic!("amulet");
        };
        assert_eq!(cards.len(), 2);
        assert!(
            cards
                .iter()
                .all(|card| !card.handle.as_str().contains("start"))
        );
        let other = if actor == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        assert!(matches!(game.input(other), Input::Waiting(wait) if wait.actor == actor));
        assert_eq!(
            game.submit_player(
                other,
                Command::Opening {
                    token: question.token,
                    answer: OpeningAnswer::StartAmulet(cards[0].handle.clone())
                }
            )
            .kind,
            SubmissionKind::Rejected
        );
        assert_eq!(
            game.submit_player(
                actor,
                Command::Opening {
                    token: question.token,
                    answer: OpeningAnswer::StartAmulet(cards[0].handle.clone())
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        old.push(cards[1].handle.clone());
        let observation = game.observe(other).unwrap();
        let field = observation
            .players
            .iter()
            .find(|player| player.player == actor)
            .unwrap()
            .zones
            .iter()
            .find(|zone| zone.kind == ObservedZone::Field)
            .unwrap();
        assert_eq!(field.count, 1);
        assert_eq!(field.cards.len(), 0);
    }
    assert_eq!(
        question(&game, Viewer::P2).shape,
        QuestionShape::Opening(OpeningQuestion::FirstPlayer)
    );
    let chooser = question(&game, Viewer::P2);
    assert_eq!(
        game.submit_player(
            Viewer::P2,
            Command::Opening {
                token: chooser.token,
                answer: OpeningAnswer::FirstPlayer(Viewer::P2)
            }
        )
        .kind,
        SubmissionKind::Applied
    );
    for actor in [Viewer::P2, Viewer::P1] {
        let current = question(&game, actor);
        assert_eq!(
            game.submit_player(
                actor,
                Command::Opening {
                    token: current.token,
                    answer: OpeningAnswer::Mulligan(vec![old[0].clone(); 4])
                }
            )
            .kind,
            SubmissionKind::Rejected
        );
        assert_eq!(
            game.submit_player(
                actor,
                Command::Opening {
                    token: current.token,
                    answer: OpeningAnswer::Keep
                }
            )
            .kind,
            SubmissionKind::Applied
        );
    }
    for actor in [Viewer::P1, Viewer::P2] {
        for player in &game.observe(actor).unwrap().players {
            let field = player
                .zones
                .iter()
                .find(|zone| zone.kind == ObservedZone::Field)
                .unwrap();
            assert_eq!(field.cards.len(), 1);
        }
    }
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
