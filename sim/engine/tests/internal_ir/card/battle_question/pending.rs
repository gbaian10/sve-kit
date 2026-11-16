use super::*;
use crate::player_pipeline::question;
use sve_battle_contract::{SourceRef, observation::ProcessingStage};
use sve_engine::game::player::{DecisionShape, QuestionPurpose};

fn attack(actor: Viewer) -> Game {
    let mut initial = setup(actor.player().seat, json!([]));
    let me = if actor == Viewer::P1 { "P1" } else { "P2" };
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["zones"]["hand"] = json!([]);
    initial["players"][me]["zones"]["field"] = json!([
        {"id":"attacker","card":"unit-follower","state":{"entered_this_turn":false}}
    ]);
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-pending-order",
    )
    .unwrap();
    game.load_internal_card(
        "unit-follower",
        &card(vec![
            timing::on_attack(ability(1, vec![])),
            timing::on_attack(ability(2, vec![])),
        ]),
    )
    .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    let target = if actor == Viewer::P1 {
        "P2.leader"
    } else {
        "P1.leader"
    };
    game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":target}))
        .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn pending_order_uses_the_completed_attack_and_native_answer_in_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = attack(actor);
        let native = question(&game, actor);
        let QuestionShape::Decision(DecisionShape::Pending(choices)) = &native.shape else {
            panic!("native pending order")
        };
        assert_eq!(choices.len(), 2);
        assert_eq!(native.purpose, QuestionPurpose::RuleChoice);
        let before = exact_authority(&game);
        let projection = project(&game, actor, "pending-attack");
        let projected = projection.question();
        assert_eq!(projected.usage, wire::QuestionUse::PendingOrder);
        assert_eq!(projected.purpose, wire::QuestionPurpose::RuleChoice);
        assert_eq!(projected.processing.stage, ProcessingStage::PendingOrder);
        assert_eq!(projected.processing, projection.observation().processing[0]);
        assert!(projected.is_coherent());
        assert_eq!(projected.rules, None);
        let wire::QuestionShape::Choose {
            options,
            bounds,
            earlier,
            ..
        } = &projected.shape
        else {
            panic!("choose one occurrence")
        };
        assert_eq!(options.len(), 2);
        assert_eq!(bounds.min.get(), 1);
        assert_eq!(bounds.max.unwrap().get(), 1);
        assert_eq!(earlier.as_slice(), []);
        let wire::ChoiceValue::Occurrence {
            source,
            controller,
            event,
            position,
        } = &options[1].value
        else {
            panic!("typed occurrence")
        };
        assert_eq!(*controller, seat(actor));
        assert_eq!(
            *event,
            wire::TriggerEvent::Attack {
                ordinal: sve_battle_contract::Nat::from(1_u32)
            }
        );
        assert_eq!(position.unwrap().get(), 0);
        assert!(matches!(source, SourceRef::Card {
            printing_id, face: Some(face), line: Some(line), holder: Some(_),
        } if printing_id == "printing-follower" && face.get() == 1 && line.get() == 2));
        let command = projection
            .answer(
                actor,
                &projected.id,
                &WireAnswer::Choose {
                    chosen: vec![options[1].id.clone()],
                },
            )
            .unwrap();
        assert_eq!(
            command,
            Command::Answer {
                token: native.token,
                answer: Answer::Pending(choices[1].handle)
            }
        );
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        finish_pending(&mut game, actor).unwrap();
    }
}

#[test]
fn pending_answers_reject_foreign_missing_repeated_wrong_seat_and_stale_choices() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = attack(actor);
        let projection = project(&game, actor, "pending-validation");
        let wire::QuestionShape::Choose { options, .. } = &projection.question().shape else {
            panic!("pending choices")
        };
        let id = &projection.question().id;
        let answer = WireAnswer::Choose {
            chosen: vec![options[0].id.clone()],
        };
        let before = exact_authority(&game);
        for rejected in [
            WireAnswer::Choose { chosen: vec![] },
            WireAnswer::Choose {
                chosen: vec![options[0].id.clone(); 2],
            },
            WireAnswer::Choose {
                chosen: options.iter().map(|option| option.id.clone()).collect(),
            },
            WireAnswer::Choose {
                chosen: vec![OptionId::new("foreign:c0".into())],
            },
            WireAnswer::Quick { option: None },
        ] {
            assert_eq!(projection.answer(actor, id, &rejected), Err(AnswerError));
        }
        assert_eq!(
            projection.answer(opposite(actor), id, &answer),
            Err(AnswerError)
        );
        assert_eq!(
            projection.answer(actor, &QuestionId::new("wrong".into()), &answer),
            Err(AnswerError)
        );
        let other_projection = project(&game, actor, "pending-different-id");
        assert_eq!(
            other_projection.answer(actor, &other_projection.question().id, &answer),
            Err(AnswerError)
        );
        let command = projection.answer(actor, id, &answer).unwrap();
        assert_eq!(
            game.submit_player(opposite(actor), command.clone()).kind,
            SubmissionKind::Rejected
        );
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            game.submit_player(actor, command.clone()).kind,
            SubmissionKind::Applied
        );
        let after = exact_authority(&game);
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Rejected
        );
        assert_eq!(exact_authority(&game), after);
    }
}

fn private_grant(actor: Viewer, number: &str, secret: &str) -> Game {
    let me = if actor == Viewer::P1 { "P1" } else { "P2" };
    let them = if actor == Viewer::P1 { "P2" } else { "P1" };
    let mut initial = setup(actor.player().seat, json!([]));
    initial["turn"]["active"] = json!(me);
    initial["players"][me]["zones"]["field"] = json!([
        {"id":"recipient","card":"unit-follower","state":{"entered_this_turn":false}}
    ]);
    initial["players"][me]["zones"]["hand"] = json!([{"id":"giver","card":number}]);
    initial["players"][them]["zones"]["field"] = json!([]);
    initial["players"][them]["zones"]["hand"] = json!([{"id":"secret","card":secret}]);
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-private-pending",
    )
    .unwrap();
    for key in ["unit-follower", "unit-amulet", "unit-spell"] {
        let program = if key == number {
            source_provenance::granting(number, true)
        } else {
            card(vec![])
        };
        game.load_internal_card(key, &program).unwrap();
    }
    resolved(
        game.activate_internal_card("giver", AbilityId(1), actor.player())
            .unwrap(),
    );
    game.resume_internal_card().unwrap();
    game.internal_rule_action(
        &json!({"do":"attack","attacker":"recipient","target":format!("{them}.leader")}),
    )
    .unwrap();
    game.advance_player_input();
    game
}

#[test]
fn private_pending_sources_preserve_both_complete_views_in_both_seats() {
    let references = &board_fixture::fixture().unwrap().references;
    for actor in [Viewer::P1, Viewer::P2] {
        let left = private_grant(actor, "unit-follower", "unit-spell");
        let right = private_grant(actor, "unit-amulet", "unit-spell");
        let before = exact_authority(&left);
        let native_left = question(&left, actor);
        let native_right = question(&right, actor);
        let QuestionShape::Decision(DecisionShape::Pending(choices)) = &native_left.shape else {
            panic!("actual private pending")
        };
        assert_eq!(choices.len(), 1);
        assert_ne!(
            native_left.shape, native_right.shape,
            "the authorized occurrence identity changes"
        );
        let waiting = opposite(actor);
        assert!(matches!(left.input(waiting), Input::Waiting(_)));
        assert_eq!(left.input(waiting), right.input(waiting));
        let left_waiting = left.battle_observation(waiting, references).unwrap();
        assert_eq!(
            left_waiting,
            right.battle_observation(waiting, references).unwrap()
        );
        assert_eq!(left_waiting.pending.as_slice(), []);
        assert_eq!(left_waiting.processing.as_slice(), []);
        for game in [&left, &right] {
            assert!(
                game.battle_question(waiting, &QuestionId::new("waiting".into()), references)
                    .unwrap()
                    .is_none()
            );
        }
        let other_secret = private_grant(actor, "unit-follower", "unit-amulet");
        let first = project(&left, actor, "private-order");
        let second = project(&other_secret, actor, "private-order");
        assert_eq!(first.observation(), second.observation());
        assert_eq!(first.question(), second.question());
        assert_eq!(exact_authority(&left), before);
    }
}

#[test]
fn pending_departed_source_keeps_its_authorized_face_without_a_new_holder() {
    let program = card(vec![
        triggers::lastword(ability(1, vec![])),
        ability(
            2,
            vec![movement::put(
                2,
                Selector::Objects {
                    objects: vec![ObjectOperand::SourceHolder {}],
                },
                OwnerZone::Cemetery,
            )],
        ),
    ]);
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = board_fixture::board(actor, "unit-spell", &program).unwrap();
        resolved(
            game.activate_internal_card("s", AbilityId(2), actor.player())
                .unwrap(),
        );
        game.resume_internal_card().unwrap();
        game.advance_player_input();
        let before = exact_authority(&game);
        let projection = project(&game, actor, "departed-pending");
        let wire::QuestionShape::Choose { options, .. } = &projection.question().shape else {
            panic!("lastwords pending")
        };
        assert!(matches!(&options[0].value, wire::ChoiceValue::Occurrence {
            source: SourceRef::Card { printing_id, face: Some(face), holder: None, .. },
            event: wire::TriggerEvent::LeaveField, ..
        } if printing_id == "printing-follower" && face.get() == 1));
        assert_eq!(exact_authority(&game), before);
        assert_eq!(
            game.submit_player(
                actor,
                projection
                    .answer(
                        actor,
                        &projection.question().id,
                        &WireAnswer::Choose {
                            chosen: vec![options[0].id.clone()],
                        }
                    )
                    .unwrap()
            )
            .kind,
            SubmissionKind::Applied
        );
    }
}

fn finish_pending(game: &mut Game, actor: Viewer) -> Option<()> {
    let remaining = project(game, actor, "pending-last");
    let wire::QuestionShape::Choose { options, .. } = &remaining.question().shape else {
        return None;
    };
    assert_eq!(
        options.len(),
        1,
        "the final pending remains a singleton choice"
    );
    let command = remaining
        .answer(
            actor,
            &remaining.question().id,
            &WireAnswer::Choose {
                chosen: vec![options[0].id.clone()],
            },
        )
        .unwrap();
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Applied,
        "the final native occurrence resolves"
    );
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "no empty pending question is issued"
    );
    Some(())
}
