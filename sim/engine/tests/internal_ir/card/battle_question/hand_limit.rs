use super::*;
use crate::{board_fixtures, player_pipeline};
use sve_battle_contract::{Nat, RoleChoice, observation::ProcessingStage};
use sve_engine::game::player::events::{EventKind, MovementMethod};
use sve_engine::game::player::{ClosingAnswer, ClosingQuestion, DecisionShape, QuestionPurpose};

#[path = "hand_limit/order.rs"]
mod order;

fn make_closing(actor: Viewer, hand_size: u32, secret: &str) -> Game {
    make_closing_with_hand(
        actor,
        &vec!["unit-amulet"; usize::try_from(hand_size).unwrap()],
        secret,
    )
}

fn make_closing_with_hand(actor: Viewer, hand: &[&str], secret: &str) -> Game {
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    let (own, other) = if actor == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["hand"] = json!(
        hand.iter()
            .enumerate()
            .map(|(index, number)| json!({"id":format!("physical-hand-{index}"),"card":number}))
            .collect::<Vec<_>>()
    );
    initial["players"][other]["zones"]["hand"] = json!([{"id":"physical-secret","card":secret}]);
    initial["players"][other]["zones"]["deck"] = json!([
        {"id":"physical-next-0","card":"unit-spell"}, {"id":"physical-next-1","card":"unit-spell"}
    ]);
    for player in [own, other] {
        initial["players"][player]["zones"]["field"] = json!([]);
    }
    let mut game = Game::new(
        Arc::clone(&board_fixture::fixture().unwrap().catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-hand-limit",
    )
    .unwrap();
    for number in ["unit-follower", "unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.advance_player_input();
    let end = player_pipeline::end_command(&player_pipeline::question(&game, actor));
    assert_eq!(
        game.submit_player(actor, end).kind,
        SubmissionKind::Applied,
        "the active seat enters the real end phase"
    );
    if let Input::Question(question) = game.input(opposite(actor))
        && matches!(
            question.shape,
            QuestionShape::Decision(DecisionShape::Quick { .. })
        )
    {
        assert_eq!(
            game.submit_player(
                opposite(actor),
                Command::Answer {
                    token: question.token,
                    answer: Answer::Quick(None),
                }
            )
            .kind,
            SubmissionKind::Applied,
            "passing the real Quick window reaches hand limit"
        );
    }
    game
}

fn candidates(projection: &QuestionProjection) -> Option<&[wire::Candidate]> {
    if let wire::QuestionShape::Select { selection } = &projection.question().shape {
        Some(&selection.candidates)
    } else {
        None
    }
}

const fn select(selected: Vec<OptionId>) -> WireAnswer {
    WireAnswer::Select {
        selected,
        groups: None,
        roles: vec![],
    }
}

fn assert_candidates(
    projection: &QuestionProjection,
    cards: &[CardObservation],
    actor: Viewer,
    count: u64,
) -> Option<()> {
    let wire::QuestionShape::Select { selection } = &projection.question().shape else {
        return None;
    };
    assert_eq!(
        selection.candidates.len(),
        cards.len(),
        "the complete native hand is projected"
    );
    assert_eq!(
        selection.bounds,
        wire::Bounds {
            min: Nat::try_from(count).ok()?,
            max: Some(Nat::try_from(count).ok()?)
        },
        "the native excess is the exact selection cardinality"
    );
    let hand = &projection
        .observation()
        .players
        .iter()
        .find(|player| player.seat == seat(actor))?
        .zones
        .hand;
    assert_eq!(
        hand.count.get(),
        u64::try_from(cards.len()).ok()?,
        "the native and board hand counts agree"
    );
    assert_eq!(
        hand.cards.len(),
        cards.len(),
        "every own-hand member is available"
    );
    for (candidate, native) in selection.candidates.iter().zip(cards) {
        let Some(Subject::Card {
            card,
            object: Some(handle),
        }) = &candidate.subject
        else {
            return None;
        };
        let object = projection.observation().objects.get(handle)?;
        let member = hand.cards.iter().find(|member| &member.object == handle)?;
        assert_eq!(
            handle.as_str(),
            native.handle.as_str(),
            "the native object capability is preserved"
        );
        assert_eq!(
            card, &object.effective_face,
            "the candidate shares the board face"
        );
        assert_eq!(
            candidate.controller,
            Some(object.controller),
            "the candidate shares the board controller"
        );
        assert_eq!(
            candidate.origin,
            Some(wire::Origin {
                player: seat(actor),
                zone: Zone::Hand,
                position: member.position
            }),
            "the candidate shares the authorized hand position"
        );
    }
    Some(())
}

fn assert_discarded_hand(game: &Game, actor: Viewer, cards: &[CardObservation], count: u64) {
    let after = game
        .battle_observation(actor, &board_fixture::fixture().unwrap().references)
        .unwrap();
    let player = after
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .unwrap();
    assert_eq!(
        player.zones.hand.count.get(),
        7,
        "the real discard reaches the hand limit"
    );
    assert_eq!(
        player.zones.cemetery.count.get(),
        count,
        "all selected excess cards reach the cemetery"
    );
    assert_eq!(
        player
            .zones
            .hand
            .cards
            .iter()
            .map(|member| member.object.as_str())
            .collect::<Vec<_>>(),
        cards
            .iter()
            .take(7)
            .map(|card| card.handle.as_str())
            .collect::<Vec<_>>(),
        "discarding the chosen last members preserves every unselected hand capability"
    );
}

#[test]
fn wire_hand_limit_round_trips_the_exact_excess_for_both_seats() {
    for actor in [Viewer::P1, Viewer::P2] {
        for hand_size in [8_u32, 9] {
            let mut game = make_closing(actor, hand_size, "unit-follower");
            let native = player_pipeline::question(&game, actor);
            let QuestionShape::Closing(ClosingQuestion::Discard { cards, count }) = native.shape
            else {
                panic!("native hand-limit question")
            };
            assert_eq!(native.purpose, QuestionPurpose::RuleChoice);
            assert_eq!(count, u64::from(hand_size - 7));
            assert!(
                game.battle_observation(actor, &board_fixture::fixture().unwrap().references)
                    .is_ok(),
                "the native closing board is independently projectable"
            );
            let before = exact_authority(&game);
            let projection = project(&game, actor, "host-hand-limit");
            assert!(
                exact_authority(&game) == before,
                "projection cannot advance closing"
            );
            assert_eq!(
                assert_candidates(&projection, &cards, actor, count),
                Some(())
            );
            assert_eq!(
                projection.observation(),
                &game
                    .battle_observation(actor, &board_fixture::fixture().unwrap().references)
                    .unwrap()
            );
            assert_eq!(
                projection.question().usage,
                wire::QuestionUse::HandLimitDiscard
            );
            assert_eq!(
                projection.question().processing,
                projection.observation().processing[0]
            );
            assert_eq!(
                projection.question().processing.stage,
                Some(ProcessingStage::Closing)
            );
            assert!(projection.question().is_coherent());
            let selected = candidates(&projection)
                .unwrap()
                .iter()
                .rev()
                .take(usize::try_from(count).unwrap())
                .map(|candidate| candidate.id.clone())
                .collect();
            let command = projection
                .answer(actor, &projection.question().id, &select(selected))
                .unwrap();
            let Command::Closing {
                token,
                answer: ClosingAnswer::Discard(handles),
            } = &command
            else {
                panic!("native closing discard answer")
            };
            assert_eq!(*token, native.token);
            assert_eq!(
                *handles,
                cards
                    .iter()
                    .rev()
                    .take(usize::try_from(count).unwrap())
                    .map(|card| card.handle.clone())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                game.submit_player(actor, command).kind,
                SubmissionKind::Applied
            );
            assert_discarded_hand(&game, actor, &cards, count);
        }
    }
}

#[test]
fn wire_hand_limit_rejects_wrong_sets_and_retired_native_tokens() {
    let actor = Viewer::P1;
    let mut game = make_closing(actor, 9, "unit-spell");
    let projection = project(&game, actor, "host-hand-limit");
    let members = candidates(&projection).unwrap();
    assert!(
        members.len() > 2,
        "invalid-set cases need three distinct candidates"
    );
    let first = members[0].id.clone();
    let second = members[1].id.clone();
    let before = exact_authority(&game);
    for answer in [
        select(vec![]),
        select(vec![first.clone()]),
        select(vec![first.clone(), second.clone(), members[2].id.clone()]),
        select(vec![first.clone(), first.clone()]),
        select(vec![first.clone(), OptionId::new("foreign".into())]),
        WireAnswer::Select {
            selected: vec![first.clone(), second.clone()],
            groups: Some(vec![]),
            roles: vec![],
        },
        WireAnswer::Select {
            selected: vec![first.clone(), second.clone()],
            groups: None,
            roles: vec![RoleChoice {
                role: Nat::from(1_u32),
                selected: vec![first.clone()],
            }],
        },
        WireAnswer::Mulligan {
            redraw: false,
            bottom_order: vec![],
        },
    ] {
        assert!(matches!(
            projection.answer(actor, &projection.question().id, &answer),
            Err(AnswerError)
        ));
    }
    let valid = select(vec![second, first]);
    for (viewer, question_id) in [
        (Viewer::P2, projection.question().id.clone()),
        (actor, QuestionId::new("retired".into())),
    ] {
        assert!(matches!(
            projection.answer(viewer, &question_id, &valid),
            Err(AnswerError)
        ));
    }
    let another = project(&game, actor, "another-occurrence");
    assert!(matches!(
        another.answer(actor, &another.question().id, &valid),
        Err(AnswerError)
    ));
    assert!(
        exact_authority(&game) == before,
        "invalid wire answers preserve the whole authority"
    );
    let command = projection
        .answer(actor, &projection.question().id, &valid)
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P2, command.clone()).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == before,
        "the authority still rejects another actor"
    );
    assert_eq!(
        game.submit_player(actor, command.clone()).kind,
        SubmissionKind::Applied
    );
    let after = exact_authority(&game);
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == after,
        "the authority rejects a stale native command"
    );
}

#[test]
fn wire_hand_limit_question_and_board_do_not_depend_on_the_other_hand() {
    for actor in [Viewer::P1, Viewer::P2] {
        let mut projections = Vec::new();
        for secret in ["unit-spell", "unit-follower"] {
            let game = make_closing(actor, 9, secret);
            let projection = project(&game, actor, "host-hand-limit");
            assert!(
                game.battle_question(
                    opposite(actor),
                    &QuestionId::new("waiting".into()),
                    &board_fixture::fixture().unwrap().references
                )
                .unwrap()
                .is_none()
            );
            let other = projection
                .observation()
                .players
                .iter()
                .find(|player| player.seat == seat(opposite(actor)))
                .unwrap();
            assert_eq!(other.zones.hand.count.get(), 1);
            assert!(
                other.zones.hand.cards.is_empty(),
                "the viewer cannot identify the other hand members"
            );
            let serialized =
                serde_json::to_string(&(projection.observation(), projection.question())).unwrap();
            for private in ["physical-", "token", "serial", "epoch"] {
                assert!(!serialized.contains(private));
            }
            projections.push(projection);
        }
        assert_eq!(projections[0].observation(), projections[1].observation());
        assert_eq!(projections[0].question(), projections[1].question());
    }
}
