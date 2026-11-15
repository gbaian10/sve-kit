use super::battle_pending::fixture;
use super::*;
use sve_battle_contract::{MAX_SAFE_INTEGER, observation as wire};
use sve_engine::battle::ProjectionError;
use sve_engine::game::player::{
    Command, DecisionShape, Input, OpeningAnswer, QuestionShape, SubmissionKind, Viewer,
};

fn game(initial: &Value) -> Game {
    Game::new(
        Arc::clone(&fixture().catalog),
        initial,
        &Value::Null,
        &Value::Null,
        "wire-turn",
    )
    .unwrap()
}

fn view(game: &Game) -> wire::TurnView {
    let before = exact_authority(game);
    let first = game.battle_turn(Viewer::P1).unwrap();
    assert_eq!(
        first,
        game.battle_turn(Viewer::P2).unwrap(),
        "turn facts are public to both seats"
    );
    assert_eq!(
        exact_authority(game),
        before,
        "turn projection must not mutate authority state"
    );
    first
}

fn answering(game: &Game) -> Viewer {
    [Viewer::P1, Viewer::P2]
        .into_iter()
        .find(|viewer| matches!(game.input(*viewer), Input::Question(_)))
        .unwrap()
}

#[test]
fn wire_turn_view_opening_has_no_invented_turn_player_and_keeps_the_selected_first_player() {
    let mut players = json!({});
    for seat in ["P1", "P2"] {
        players[seat] = json!({
            "construction":"class", "leader":{"class":"ニュートラル"},
            "deck_list":(0..8).map(|i| json!({"id":format!("{seat}-{i}"),"card":"unit-spell"})).collect::<Vec<_>>(),
            "evolve_deck_list":[]
        });
    }
    for first in [Viewer::P1, Viewer::P2] {
        let mut game = game(&json!({"pregame":true,"players":players}));
        for viewer in [Viewer::P1, Viewer::P2] {
            assert_eq!(game.observe(viewer).unwrap().turn.active, None);
        }
        assert_eq!(
            serde_json::to_value(view(&game)).unwrap(),
            json!({
                "active":null,"first_player":null,"phase":null,"elapsed_turns":[0,0]
            })
        );
        let chooser = answering(&game);
        let question = player_pipeline::question(&game, chooser);
        assert_eq!(
            game.submit_player(
                chooser,
                Command::Opening {
                    token: question.token,
                    answer: OpeningAnswer::FirstPlayer(first)
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        let expected_seat = if first == Viewer::P1 { "p1" } else { "p2" };
        assert_eq!(
            serde_json::to_value(view(&game)).unwrap(),
            json!({
                "active":expected_seat,"first_player":expected_seat,"phase":null,"elapsed_turns":[0,0]
            })
        );
        let second = if first == Viewer::P1 {
            Viewer::P2
        } else {
            Viewer::P1
        };
        for actor in [first, second] {
            let question = player_pipeline::question(&game, actor);
            assert_eq!(
                game.submit_player(
                    actor,
                    Command::Opening {
                        token: question.token,
                        answer: OpeningAnswer::Keep
                    }
                )
                .kind,
                SubmissionKind::Applied
            );
        }
        assert_eq!(
            serde_json::to_value(view(&game)).unwrap(),
            json!({
                "active":expected_seat,"first_player":expected_seat,"phase":"main",
                "elapsed_turns":if first==Viewer::P1 { [1,0] } else { [0,1] }
            })
        );
    }
}

#[test]
fn wire_turn_view_preserves_each_phase_and_checks_each_seat_count_without_secret_dependence() {
    for phase in ["start", "main", "end"] {
        for counts in [
            [0, 0],
            [MAX_SAFE_INTEGER, 2],
            [3, MAX_SAFE_INTEGER],
            [MAX_SAFE_INTEGER + 1, 0],
            [0, MAX_SAFE_INTEGER + 1],
            [u64::MAX, 0],
            [0, u64::MAX],
        ] {
            let mut previous = None;
            for secret in ["unit-spell", "unit-amulet"] {
                let mut initial = setup(1, json!([]));
                initial["turn"] = json!({"active":"P2","first_player":"P1","phase":phase,"elapsed_turns":{"P1":counts[0],"P2":counts[1]}});
                initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"secret","card":secret}]);
                let game = game(&initial);
                let before = exact_authority(&game);
                for viewer in [Viewer::P1, Viewer::P2] {
                    let projected = game.battle_turn(viewer);
                    if counts.into_iter().any(|count| count > MAX_SAFE_INTEGER) {
                        assert_eq!(projected, Err(ProjectionError::Range));
                    } else {
                        let value = projected.unwrap();
                        assert_eq!(
                            serde_json::to_value(value).unwrap(),
                            json!({
                                "active":"p2","first_player":"p1","phase":phase,"elapsed_turns":counts
                            })
                        );
                        if let Some(previous) = &previous {
                            assert_eq!(&value, previous);
                        }
                        previous = Some(value);
                    }
                }
                assert_eq!(exact_authority(&game), before);
            }
        }
    }
    let mut neutral = setup(1, json!([]));
    neutral["turn"] = json!({"active":"P2","phase":"main"});
    assert_eq!(
        serde_json::to_value(view(&game(&neutral))).unwrap(),
        json!({
            "active":"p2","first_player":null,"phase":"main","elapsed_turns":[0,0]
        })
    );
    let mut initial = setup(1, json!([]));
    initial["turn"]["phase"] = json!("unknown-phase");
    assert_eq!(
        game(&initial).battle_turn(Viewer::P1),
        Err(ProjectionError::Unavailable)
    );
}

#[test]
fn wire_turn_view_refuses_malformed_authority_fields_instead_of_defaulting_them() {
    for (field, value) in [
        ("elapsed_turns", json!(null)),
        ("elapsed_turns", json!([])),
        ("elapsed_turns", json!({"P1":-1,"P2":2})),
        ("elapsed_turns", json!({"P1":3,"P2":1.5})),
        ("elapsed_turns", json!({"P1":"3","P2":2})),
        ("elapsed_turns", json!({"P1":3,"P2":true})),
        ("elapsed_turns", json!({"P1":null,"P2":2})),
        ("active", json!(0)),
        ("first_player", json!(false)),
        ("phase", json!(0)),
        ("phase", json!([])),
    ] {
        let mut initial = setup(1, json!([]));
        initial["turn"][field] = value;
        let game = game(&initial);
        let before = exact_authority(&game);
        for viewer in [Viewer::P1, Viewer::P2] {
            game.observe(viewer).unwrap_err();
            assert_eq!(game.battle_turn(viewer), Err(ProjectionError::Unavailable));
        }
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn wire_turn_view_tracks_real_closing_quick_and_next_turns_without_guessing_alternation() {
    for (actor, schedule, expected) in [
        (
            Viewer::P1,
            "normal",
            [(Viewer::P1, "p2", [3, 3]), (Viewer::P2, "p1", [4, 3])],
        ),
        (
            Viewer::P2,
            "normal",
            [(Viewer::P2, "p1", [4, 2]), (Viewer::P1, "p2", [4, 3])],
        ),
        (
            Viewer::P1,
            "extra",
            [(Viewer::P1, "p1", [4, 2]), (Viewer::P1, "p2", [4, 3])],
        ),
        (
            Viewer::P2,
            "extra",
            [(Viewer::P2, "p2", [3, 3]), (Viewer::P2, "p1", [4, 3])],
        ),
        (
            Viewer::P1,
            "skip",
            [(Viewer::P1, "p1", [4, 2]), (Viewer::P1, "p2", [4, 3])],
        ),
        (
            Viewer::P2,
            "skip",
            [(Viewer::P2, "p2", [3, 3]), (Viewer::P2, "p1", [4, 3])],
        ),
    ] {
        let mut initial = setup(actor.player().seat, json!([]));
        let active = if actor == Viewer::P1 { "P1" } else { "P2" };
        initial["turn"]["active"] = json!(active);
        if schedule == "extra" {
            initial["semantic_state"]["turn_schedule"] =
                json!({"extra":[{"player":active,"count":1}],"skipped":[]});
        } else if schedule == "skip" {
            let skipped = if actor == Viewer::P1 { "P2" } else { "P1" };
            initial["semantic_state"]["turn_schedule"] =
                json!({"extra":[],"skipped":[{"player":skipped,"count":1}]});
        } else {
            assert_eq!(schedule, "normal");
        }
        for seat in ["P1", "P2"] {
            initial["players"][seat]["zones"]["deck"] = json!([
                {"id":format!("{seat}-d1"),"card":"unit-spell"},
                {"id":format!("{seat}-d2"),"card":"unit-spell"}
            ]);
        }
        let mut game = game(&initial);
        game.advance_player_input();
        for (current, next, counts) in expected {
            let question = player_pipeline::question(&game, current);
            assert_eq!(
                game.submit_player(current, player_pipeline::end_command(&question))
                    .kind,
                SubmissionKind::Applied
            );
            assert_eq!(view(&game).phase, Some(wire::Phase::End));
            let quick_actor = answering(&game);
            let question = player_pipeline::question(&game, quick_actor);
            assert!(matches!(
                question.shape,
                QuestionShape::Decision(DecisionShape::Quick { .. })
            ));
            assert_eq!(
                game.submit_player(
                    quick_actor,
                    Command::Answer {
                        token: question.token,
                        answer: Answer::Quick(None)
                    }
                )
                .kind,
                SubmissionKind::Applied
            );
            assert_eq!(
                serde_json::to_value(view(&game)).unwrap(),
                json!({
                    "active":next,"first_player":"p1","phase":"main","elapsed_turns":counts
                })
            );
        }
    }
}
