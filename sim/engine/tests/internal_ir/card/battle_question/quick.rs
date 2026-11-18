use super::*;
use crate::player_pipeline::question;
use sve_battle_contract::{SourceRef, observation::ProcessingStage};
use sve_engine::game::player::{
    DecisionShape, QuestionPurpose, QuickChoice, observation::TextProvenance,
};

fn position(actor: Viewer, attack: bool, hand: &[&str], ability_option: bool) -> Game {
    with_secret(actor, attack, hand, ability_option, "unit-amulet")
}

fn with_secret(
    actor: Viewer,
    attack: bool,
    hand: &[&str],
    ability_option: bool,
    secret: &str,
) -> Game {
    let active = opposite(actor);
    let me = if actor == Viewer::P1 { "P1" } else { "P2" };
    let them = if actor == Viewer::P1 { "P2" } else { "P1" };
    let mut initial = setup(active.player().seat, json!([]));
    initial["turn"]["active"] = json!(them);
    for owner in [me, them] {
        initial["players"][owner]["pp"] = json!({"current":4,"max":4});
        initial["players"][owner]["zones"]["deck"] = json!([
            {"id":format!("deck-{owner}-0"),"card":"unit-amulet"},
            {"id":format!("deck-{owner}-1"),"card":"unit-amulet"}
        ]);
        initial["players"][owner]["zones"]["hand"] = json!([]);
    }
    initial["players"][them]["zones"]["hand"] = json!([{"id":"other-secret","card":secret}]);
    initial["players"][me]["zones"]["hand"] = json!(
        hand.iter()
            .enumerate()
            .map(|(i, card)| json!({"id":format!("quick-{i}"),"card":card}))
            .collect::<Vec<_>>()
    );
    initial["players"][them]["zones"]["field"] = json!([
        {"id":"attacker","card":"unit-follower","state":{"entered_this_turn":false}}
    ]);
    if ability_option {
        initial["players"][me]["zones"]["field"] = json!([
            {"id":"ability","card":"unit-follower"}
        ]);
    }
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-quick",
    )
    .unwrap();
    let spell = card(vec![play::spell(vec![]), super::super::quick::permit()]);
    let follower = if ability_option {
        card(vec![super::super::quick::quick_activated(ability(
            1,
            vec![],
        ))])
    } else {
        card(vec![])
    };
    game.load_internal_card("unit-spell", &spell).unwrap();
    game.load_internal_card("unit-follower", &follower).unwrap();
    game.load_internal_card("unit-amulet", &card(vec![]))
        .unwrap();
    let step = if attack {
        game.internal_rule_action(
            &json!({"do":"attack","attacker":"attacker","target":format!("{me}.leader")}),
        )
        .unwrap()
    } else {
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
    };
    assert_eq!(
        step.outcome, "resolved",
        "the native rules action reaches its Quick stop"
    );
    game.advance_player_input();
    game
}

#[test]
fn empty_quick_windows_keep_explicit_pass_in_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        for attack in [false, true] {
            let mut game = position(actor, attack, &[], false);
            let native = question(&game, actor);
            assert!(
                matches!(&native.shape, QuestionShape::Decision(DecisionShape::Quick { options, .. }) if options.is_empty())
            );
            assert_eq!(native.purpose, QuestionPurpose::QuickWindow);
            let before = exact_authority(&game);
            let projection = project(&game, actor, "host-quick");
            assert!(exact_authority(&game) == before, "projection is read-only");
            assert_eq!(
                projection.question().purpose,
                wire::QuestionPurpose::QuickWindow
            );
            assert_eq!(projection.question().usage, wire::QuestionUse::Quick);
            assert_eq!(
                projection.question().processing.stage,
                Some(ProcessingStage::Quick)
            );
            let (window, options) = wire_options(&projection).unwrap();
            assert_eq!(
                window,
                if attack {
                    wire::QuickWindow::Attack
                } else {
                    wire::QuickWindow::EndPhase
                }
            );
            assert_eq!(options, []);
            let answer = WireAnswer::Quick { option: None };
            let command = projection
                .answer(actor, &projection.question().id, &answer)
                .unwrap();
            assert_eq!(
                command,
                Command::Answer {
                    token: native.token,
                    answer: Answer::Quick(None)
                }
            );
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
            assert_eq!(
                game.submit_player(
                    actor,
                    projection
                        .answer(actor, &projection.question().id, &answer)
                        .unwrap()
                )
                .kind,
                SubmissionKind::Rejected
            );
        }
    }
}

fn wire_options(
    projection: &QuestionProjection,
) -> Option<(wire::QuickWindow, &[wire::QuickOption])> {
    if let wire::QuestionShape::Quick { window, options } = &projection.question().shape {
        Some((*window, options))
    } else {
        None
    }
}

fn native_options(game: &Game, actor: Viewer) -> Option<Vec<QuickChoice>> {
    if let QuestionShape::Decision(DecisionShape::Quick { options, .. }) =
        question(game, actor).shape
    {
        Some(options)
    } else {
        None
    }
}

#[test]
fn later_identical_quick_card_keeps_its_native_handle_and_private_origin() {
    for actor in [Viewer::P1, Viewer::P2] {
        for attack in [false, true] {
            let mut game = position(
                actor,
                attack,
                &["unit-amulet", "unit-spell", "unit-spell"],
                false,
            );
            let native = question(&game, actor);
            let original = native_options(&game, actor).unwrap();
            assert_eq!(original.len(), 2);
            let projection = project(&game, actor, "host-quick-card");
            let (_, options) = wire_options(&projection).unwrap();
            let last = options.last().unwrap();
            assert_eq!(last.action, wire::QuickAction::Play);
            assert!(matches!(&last.source, SourceRef::Card {
                printing_id, face: Some(face), line: None, holder: Some(_),
            } if printing_id=="printing-spell" && face.get()==0));
            assert_eq!(last.origin.unwrap().position.unwrap().get(), 2);
            assert_eq!(last.origin.unwrap().zone, Zone::Hand);
            let before = exact_authority(&game);
            let id = &projection.question().id;
            for (viewer, question_id, answer) in [
                (
                    opposite(actor),
                    id.clone(),
                    WireAnswer::Quick { option: None },
                ),
                (
                    actor,
                    QuestionId::new("foreign".into()),
                    WireAnswer::Quick { option: None },
                ),
                (
                    actor,
                    id.clone(),
                    WireAnswer::Quick {
                        option: Some(OptionId::new("foreign".into())),
                    },
                ),
                (actor, id.clone(), WireAnswer::Choose { chosen: vec![] }),
            ] {
                assert_eq!(
                    projection.answer(viewer, &question_id, &answer),
                    Err(AnswerError)
                );
            }
            let command = projection
                .answer(
                    actor,
                    id,
                    &WireAnswer::Quick {
                        option: Some(last.id.clone()),
                    },
                )
                .unwrap();
            assert_eq!(
                command,
                Command::Answer {
                    token: native.token,
                    answer: Answer::Quick(Some(original.last().unwrap().handle))
                }
            );
            assert_eq!(
                game.submit_player(opposite(actor), command.clone()).kind,
                SubmissionKind::Rejected
            );
            assert!(
                exact_authority(&game) == before,
                "rejected input and decoding never pay or move cards"
            );
            assert_eq!(
                game.submit_player(actor, command.clone()).kind,
                SubmissionKind::Applied
            );
            let me = if actor == Viewer::P1 { "P1" } else { "P2" };
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state[me]["pp"]["current"], 3);
            assert!(
                state[me]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("quick-2"))
            );
            assert!(
                state[me]["hand"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("quick-1"))
            );
            let after = exact_authority(&game);
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Rejected
            );
            assert!(exact_authority(&game) == after, "stale input is atomic");
        }
    }
}

#[test]
fn quick_activation_uses_authorized_source_and_the_adopted_face_map() {
    for actor in [Viewer::P1, Viewer::P2] {
        for attack in [false, true] {
            let mut game = position(actor, attack, &[], true);
            let native = question(&game, actor);
            let original = native_options(&game, actor).unwrap();
            assert_eq!(original.len(), 1);
            let source = original[0].source.as_ref().unwrap();
            assert_eq!(source.face, Some(0));
            assert_eq!(source.line, Some(1));
            assert_eq!(source.provenance, TextProvenance::Printed);
            let projection = project(&game, actor, "host-quick-ability");
            let (_, options) = wire_options(&projection).unwrap();
            let only = options.first().unwrap();
            assert_eq!(options.len(), 1);
            assert_eq!(only.action, wire::QuickAction::Ability);
            assert!(matches!(&only.source, SourceRef::Card {
                printing_id, face: Some(face), line: Some(line), holder: Some(holder),
            } if printing_id=="printing-follower" && face.get()==1 && line.get()==1
                && holder.as_str()==original[0].object.as_ref().unwrap().as_str()));
            assert_eq!(only.origin.unwrap().zone, Zone::Field);
            assert_eq!(only.origin.unwrap().position.unwrap().get(), 0);
            let command = projection
                .answer(
                    actor,
                    &projection.question().id,
                    &WireAnswer::Quick {
                        option: Some(only.id.clone()),
                    },
                )
                .unwrap();
            assert_eq!(
                command,
                Command::Answer {
                    token: native.token,
                    answer: Answer::Quick(Some(original[0].handle))
                }
            );
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
        }
    }
}

#[test]
fn private_quick_legality_and_opposing_secrets_have_both_direction_differences() {
    for actor in [Viewer::P1, Viewer::P2] {
        for attack in [false, true] {
            let many = position(actor, attack, &["unit-spell", "unit-spell"], false);
            let fewer = position(actor, attack, &["unit-spell", "unit-amulet"], false);
            assert_eq!(native_options(&many, actor).unwrap().len(), 2);
            assert_eq!(native_options(&fewer, actor).unwrap().len(), 1);
            assert_eq!(
                wire_options(&project(&many, actor, "same"))
                    .unwrap()
                    .1
                    .len(),
                2
            );
            assert_eq!(
                wire_options(&project(&fewer, actor, "same"))
                    .unwrap()
                    .1
                    .len(),
                1
            );
            let waiting = opposite(actor);
            assert_eq!(many.input(waiting), fewer.input(waiting));
            let references = &board_fixture::fixture().unwrap().references;
            assert_eq!(
                many.battle_observation(waiting, references).unwrap(),
                fewer.battle_observation(waiting, references).unwrap()
            );
            assert!(
                many.battle_question(waiting, &QuestionId::new("waiting".into()), references)
                    .unwrap()
                    .is_none()
            );
            assert!(
                fewer
                    .battle_question(waiting, &QuestionId::new("waiting".into()), references)
                    .unwrap()
                    .is_none()
            );
            let secret = with_secret(
                actor,
                attack,
                &["unit-spell", "unit-spell"],
                false,
                "unit-spell",
            );
            let left = project(&many, actor, "opponent-secret");
            let right = project(&secret, actor, "opponent-secret");
            assert_eq!(left.observation(), right.observation());
            assert_eq!(left.question(), right.question());
        }
    }
}
