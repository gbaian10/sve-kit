use super::*;
use crate::board_fixture;
use sve_battle_contract::{
    Answer as WireAnswer, OptionId, QuestionId, Seat, Subject, Zone, question as wire,
};
use sve_engine::battle::{AnswerError, ProjectionError, QuestionProjection};
use sve_engine::game::player::observation::CardObservation;
use sve_engine::game::player::{
    Command, Input, OpeningAnswer, OpeningQuestion, QuestionShape, SubmissionKind, Viewer,
};

#[path = "battle_question/end_guard.rs"]
mod end_guard;
#[path = "battle_question/evolution_discard.rs"]
mod evolution_discard;
#[path = "battle_question/evolution_form.rs"]
mod evolution_form;
#[path = "battle_question/evolution_materials.rs"]
mod evolution_materials;
#[path = "battle_question/evolution_payment.rs"]
mod evolution_payment;
#[path = "battle_question/hand_limit.rs"]
mod hand_limit;
#[path = "battle_question/pending.rs"]
mod pending;
#[path = "battle_question/quick.rs"]
mod quick;
#[path = "battle_question/start_amulet.rs"]
mod start_amulet;

fn opposite(viewer: Viewer) -> Viewer {
    if viewer == Viewer::P1 {
        Viewer::P2
    } else {
        Viewer::P1
    }
}
fn seat(viewer: Viewer) -> Seat {
    if viewer == Viewer::P1 {
        Seat::P1
    } else {
        Seat::P2
    }
}
fn make_with_catalog(chooser: Viewer, cards: [&str; 2], catalog: Arc<Catalog>) -> Game {
    let mut players = json!({});
    for (player, number) in ["P1", "P2"].into_iter().zip(cards) {
        players[player] = json!({"construction":"class","leader":{"class":"ニュートラル"},
            "deck_list":(0_u32..8).map(|index| json!({"id":format!("physical-{player}-{index}"),"card":number})).collect::<Vec<_>>(),"evolve_deck_list":[]});
    }
    let mut game = Game::new(
        catalog,
        &json!({"pregame":true,"players":players}),
        &Value::Null,
        &json!({"first_chooser":if chooser==Viewer::P1 {"P1"} else {"P2"}}),
        "wire-opening",
    )
    .unwrap();
    for number in ["unit-follower", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game
}
fn make(chooser: Viewer, cards: [&str; 2]) -> Game {
    make_with_catalog(
        chooser,
        cards,
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
    )
}
fn project(game: &Game, actor: Viewer, id: &str) -> QuestionProjection {
    game.battle_question(
        actor,
        &QuestionId::new(id.into()),
        &board_fixture::fixture().unwrap().references,
    )
    .unwrap()
    .unwrap()
}
fn first_answer(projection: &QuestionProjection, first: Viewer) -> Option<WireAnswer> {
    let wire::QuestionShape::Choose { options, .. } = &projection.question().shape else {
        return None;
    };
    let option = options.iter().find(|option| {
        option.value
            == wire::ChoiceValue::Seat {
                player: seat(first),
            }
    })?;
    Some(WireAnswer::Choose {
        chosen: vec![option.id.clone()],
    })
}
fn submit_first(game: &mut Game, chooser: Viewer, first: Viewer) {
    let projection = project(game, chooser, "host-first");
    let command = projection
        .answer(
            chooser,
            &projection.question().id,
            &first_answer(&projection, first).unwrap(),
        )
        .unwrap();
    assert_eq!(
        game.submit_player(chooser, command).kind,
        SubmissionKind::Applied,
        "a projected first-player answer must advance the real opening"
    );
}
fn hand(projection: &QuestionProjection) -> Option<&[wire::Candidate]> {
    if let wire::QuestionShape::Mulligan { hand } = &projection.question().shape {
        Some(hand)
    } else {
        None
    }
}

fn assert_hand_candidates(
    projection: &QuestionProjection,
    cards: &[CardObservation],
    actor: Viewer,
) -> Option<()> {
    let candidates = hand(projection)?;
    assert_eq!(
        candidates.len(),
        cards.len(),
        "native and wire hand lengths agree"
    );
    let hand = &projection
        .observation()
        .players
        .iter()
        .find(|player| player.seat == seat(actor))?
        .zones
        .hand;
    for (candidate, card) in candidates.iter().zip(cards.iter()) {
        let Some(Subject::Card {
            object: Some(handle),
            card: face,
        }) = &candidate.subject
        else {
            return None;
        };
        assert_eq!(
            handle.as_str(),
            card.handle.as_str(),
            "a candidate uses the actor-authorized object handle"
        );
        assert_eq!(
            face,
            &projection.observation().objects.get(handle)?.effective_face,
            "the subject uses this board object table's effective face"
        );
        let origin = candidate.origin?;
        let member = hand.cards.iter().find(|member| &member.object == handle)?;
        assert_eq!(
            origin.position, member.position,
            "the origin preserves this board hand member's position"
        );
        assert_eq!(
            origin.zone,
            Zone::Hand,
            "mulligan candidates come from the complete hand"
        );
        assert_eq!(
            origin.player,
            seat(actor),
            "the candidate belongs to this actor's hand"
        );
        assert_eq!(
            candidate.controller,
            Some(projection.observation().objects.get(handle)?.controller),
            "the controller is frozen from this board object table"
        );
    }
    Some(())
}

#[test]
fn wire_opening_questions_round_trip_first_player_for_both_seats() {
    for chooser in [Viewer::P1, Viewer::P2] {
        for first in [Viewer::P1, Viewer::P2] {
            let mut game = make(chooser, ["unit-amulet", "unit-spell"]);
            let before = exact_authority(&game);
            let Input::Question(native) = game.input(chooser) else {
                panic!("native first-player question")
            };
            let projection = project(&game, chooser, "host-first");
            assert!(
                exact_authority(&game) == before,
                "projection cannot advance the opening"
            );
            assert_eq!(
                projection.observation(),
                &game
                    .battle_observation(chooser, &board_fixture::fixture().unwrap().references)
                    .unwrap()
            );
            let question = projection.question();
            assert_eq!(question.actor, seat(chooser));
            assert_eq!(question.purpose, wire::QuestionPurpose::RuleChoice);
            assert_eq!(question.usage, wire::QuestionUse::FirstPlayer);
            assert_eq!(question.processing, projection.observation().processing[0]);
            assert_eq!(question.auto_answer, wire::AutoAnswer::Unknown);
            assert!(question.is_coherent());
            assert_eq!(
                serde_json::to_value(question).unwrap()["assistance"],
                Value::Null
            );
            assert_eq!(
                serde_json::to_value(question).unwrap()["rules"],
                Value::Null
            );
            assert!(
                game.battle_question(
                    opposite(chooser),
                    &QuestionId::new("waiting".into()),
                    &board_fixture::fixture().unwrap().references
                )
                .unwrap()
                .is_none()
            );
            let command = projection
                .answer(
                    chooser,
                    &question.id,
                    &first_answer(&projection, first).unwrap(),
                )
                .unwrap();
            let Command::Opening {
                token,
                answer: OpeningAnswer::FirstPlayer(chosen),
            } = &command
            else {
                panic!("native first-player command")
            };
            assert_eq!(
                *token, native.token,
                "the decoder binds the authority's current token"
            );
            assert_eq!(*chosen, first);
            assert_eq!(
                game.submit_player(chooser, command).kind,
                SubmissionKind::Applied
            );
        }
    }
}

#[test]
fn wire_opening_questions_round_trip_mulligan_for_both_seats() {
    for chooser in [Viewer::P1, Viewer::P2] {
        for redraw in [false, true] {
            let mut game = make(chooser, ["unit-amulet", "unit-spell"]);
            let first = opposite(chooser);
            submit_first(&mut game, chooser, first);
            for (index, actor) in [first, opposite(first)].into_iter().enumerate() {
                let before = exact_authority(&game);
                let projection = project(&game, actor, &format!("host-hand-{index}"));
                let Input::Question(native) = game.input(actor) else {
                    panic!("native opening question")
                };
                let QuestionShape::Opening(OpeningQuestion::Mulligan(cards)) = native.shape else {
                    panic!("native mulligan")
                };
                let candidates = hand(&projection).unwrap();
                assert_eq!(candidates.len(), 4);
                assert_eq!(projection.question().usage, wire::QuestionUse::Mulligan);
                assert_eq!(
                    projection.question().processing,
                    projection.observation().processing[0]
                );
                assert_eq!(
                    projection.question().processing.procedure.unwrap().actor,
                    seat(actor)
                );
                assert!(projection.question().is_coherent());
                assert_hand_candidates(&projection, &cards, actor).unwrap();
                let bottom_order = if redraw {
                    candidates
                        .iter()
                        .rev()
                        .map(|candidate| candidate.id.clone())
                        .collect()
                } else {
                    vec![]
                };
                let command = projection
                    .answer(
                        actor,
                        &projection.question().id,
                        &WireAnswer::Mulligan {
                            redraw,
                            bottom_order,
                        },
                    )
                    .unwrap();
                let Command::Opening { token, answer } = &command else {
                    panic!("native opening answer")
                };
                assert_eq!(
                    *token, native.token,
                    "the decoder preserves the actual native token"
                );
                assert_eq!(
                    answer,
                    &if redraw {
                        OpeningAnswer::Mulligan(
                            cards.iter().rev().map(|card| card.handle.clone()).collect(),
                        )
                    } else {
                        OpeningAnswer::Keep
                    }
                );
                assert!(
                    exact_authority(&game) == before,
                    "reading and decoding leave authority unchanged"
                );
                assert_eq!(
                    game.submit_player(actor, command).kind,
                    SubmissionKind::Applied
                );
            }
            assert!(
                matches!(
                    game.battle_question(
                        first,
                        &QuestionId::new("main".into()),
                        &board_fixture::fixture().unwrap().references
                    ),
                    Err(ProjectionError::Unavailable)
                ),
                "unimplemented active shapes cannot look like a supported question"
            );
        }
    }
}

#[test]
fn wire_opening_answers_bind_the_actor_and_question_id_before_decoding() {
    let actor = Viewer::P2;
    let mut game = make(actor, ["unit-amulet", "unit-spell"]);
    let projection = project(&game, actor, "bound-first");
    let answer = first_answer(&projection, Viewer::P1).unwrap();
    let before = exact_authority(&game);
    assert_eq!(
        projection.answer(actor, &QuestionId::new("wrong-question".into()), &answer),
        Err(AnswerError)
    );
    assert_eq!(
        projection.answer(opposite(actor), &projection.question().id, &answer),
        Err(AnswerError)
    );
    for answer in [
        WireAnswer::Choose { chosen: vec![] },
        WireAnswer::Choose {
            chosen: vec![OptionId::new("elsewhere:c0".into())],
        },
        WireAnswer::Choose {
            chosen: vec![OptionId::new("bound-first:c0".into()); 2],
        },
        WireAnswer::Mulligan {
            redraw: false,
            bottom_order: vec![],
        },
    ] {
        assert_eq!(
            projection.answer(actor, &projection.question().id, &answer),
            Err(AnswerError)
        );
    }
    assert!(
        exact_authority(&game) == before,
        "invalid answers cannot touch authority"
    );
    let command = projection
        .answer(actor, &projection.question().id, &answer)
        .unwrap();
    assert_eq!(
        game.submit_player(opposite(actor), command.clone()).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == before,
        "decoding or rejecting an answer cannot change authority"
    );
    assert_eq!(
        game.submit_player(actor, command.clone()).kind,
        SubmissionKind::Applied
    );
    let after = exact_authority(&game);
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Rejected,
        "the original native token is stale after the first-player answer"
    );
    assert!(
        exact_authority(&game) == after,
        "a stale native command cannot change authority"
    );
}

#[test]
fn wire_opening_mulligan_requires_a_complete_permutation() {
    let actor = Viewer::P1;
    let mut game = make(actor, ["unit-amulet", "unit-spell"]);
    submit_first(&mut game, actor, actor);
    let projection = project(&game, actor, "hand-a");
    let ids = hand(&projection)
        .unwrap()
        .iter()
        .map(|candidate| candidate.id.clone())
        .collect::<Vec<_>>();
    let before = exact_authority(&game);
    assert_eq!(
        projection.answer(
            actor,
            &projection.question().id,
            &WireAnswer::Mulligan {
                redraw: true,
                bottom_order: vec![]
            }
        ),
        Err(AnswerError),
        "redraw must return the entire hand"
    );
    for answer in [
        WireAnswer::Mulligan {
            redraw: true,
            bottom_order: ids[..3].to_vec(),
        },
        WireAnswer::Mulligan {
            redraw: true,
            bottom_order: vec![ids[0].clone(); 4],
        },
        WireAnswer::Mulligan {
            redraw: false,
            bottom_order: ids.clone(),
        },
        WireAnswer::Mulligan {
            redraw: true,
            bottom_order: vec![OptionId::new("foreign:c0".into()); 4],
        },
        WireAnswer::Choose {
            chosen: ids.clone(),
        },
    ] {
        assert_eq!(
            projection.answer(actor, &projection.question().id, &answer),
            Err(AnswerError)
        );
    }
    let second = project(&game, actor, "hand-b");
    let answer = WireAnswer::Mulligan {
        redraw: true,
        bottom_order: ids,
    };
    assert_eq!(
        second.answer(actor, &projection.question().id, &answer),
        Err(AnswerError)
    );
    assert_eq!(
        second.answer(actor, &second.question().id, &answer),
        Err(AnswerError),
        "candidates are local to the issued question ID"
    );
    assert!(
        exact_authority(&game) == before,
        "decoding or rejecting an answer cannot change authority"
    );
    let keep = projection
        .answer(
            actor,
            &projection.question().id,
            &WireAnswer::Mulligan {
                redraw: false,
                bottom_order: vec![],
            },
        )
        .unwrap();
    assert_eq!(
        game.submit_player(actor, keep.clone()).kind,
        SubmissionKind::Applied
    );
    let after = exact_authority(&game);
    assert_eq!(
        game.submit_player(actor, keep).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == after,
        "a stale native command cannot change authority"
    );
}

#[test]
fn wire_opening_projection_keeps_the_other_hand_secret_and_requires_its_catalogue() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut first_cards = ["unit-spell"; 2];
        let mut second_cards = first_cards;
        first_cards[usize::from(actor == Viewer::P1)] = "unit-amulet";
        second_cards[usize::from(actor == Viewer::P1)] = "unit-follower";
        let mut first = make(actor, first_cards);
        let mut second = make(actor, second_cards);
        submit_first(&mut first, actor, actor);
        submit_first(&mut second, actor, actor);
        let before = exact_authority(&first);
        let a = project(&first, actor, "same-stop");
        let b = project(&second, actor, "same-stop");
        assert_eq!(a.observation(), b.observation());
        assert_eq!(a.question(), b.question());
        let bytes = serde_json::to_string(&(a.observation(), a.question())).unwrap();
        assert!(
            !bytes.contains("physical-"),
            "physical IDs never enter the wire"
        );
        assert!(!bytes.contains("token") && !bytes.contains("serial") && !bytes.contains("epoch"));
        assert!(
            !format!("{a:?}").contains("vobj:"),
            "projection debug cannot expose its private lookup"
        );
        assert!(
            exact_authority(&first) == before,
            "question projection is read-only"
        );
    }
    let snapshot = format!("{}\n", board_fixture::fixture().unwrap().snapshot);
    let wrong = make_with_catalog(
        Viewer::P1,
        ["unit-amulet", "unit-spell"],
        Arc::new(board_fixture::catalog_for(&snapshot).unwrap()),
    );
    assert!(
        matches!(wrong.input(Viewer::P1), Input::Question(_)),
        "the mismatched catalogue still has a valid native opening question"
    );
    assert!(matches!(
        wrong.battle_question(
            Viewer::P1,
            &QuestionId::new("wrong-catalogue".into()),
            &board_fixture::fixture().unwrap().references
        ),
        Err(ProjectionError::Unavailable)
    ));
}
