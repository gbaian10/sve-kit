use super::*;
use crate::{board_fixtures, player_pipeline, rule_fixtures};
use sve_battle_contract::{
    CardRef, Nat, RoleChoice,
    observation::{Posture, ProcessingStage},
};
use sve_engine::battle::{BattleReferences, PrintingFaceIdentity, ReferenceInput};
use sve_engine::catalog::EngineIdentityInput;
use sve_engine::game::player::{ClosingAnswer, ClosingQuestion, DecisionShape};

fn fixture() -> (Arc<Catalog>, BattleReferences) {
    let snapshot = &board_fixture::fixture().unwrap().snapshot;
    let (_, _, document) = board_fixtures::catalog_documents(&json!("1"));
    let keywords = json!({"format":1_i32,"kind":"keyword_registry","keywords":{
        "guard":{"ja":"守護","rule":"12.8","expansion":{"op":"keyword","name":"guard"}}
    }});
    let rules = rule_fixtures::rules(
        snapshot,
        json!([rule_fixtures::title(
            "synthetic",
            "unit-follower",
            json!([])
        )]),
        json!([]),
    );
    let catalog = Arc::new(
        Catalog::from_documents_with_rules(
            snapshot,
            &keywords.to_string(),
            &[("guard".into(), document)],
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap(),
    );
    let faces = [
        ("unit-follower", 0_u64, "printing-follower", 1_u32),
        ("unit-follower", 1, "printing-follower", 0),
        ("unit-spell", 0, "printing-spell", 0),
        ("unit-amulet", 0, "printing-amulet", 0),
    ]
    .into_iter()
    .map(
        |(number, native_face, printing, face)| PrintingFaceIdentity {
            card_no: number.into(),
            native_face,
            card: CardRef {
                printing_id: printing.into(),
                face: Nat::from(face),
            },
        },
    )
    .collect();
    let references = BattleReferences::new(
        &catalog,
        ReferenceInput {
            region: "jp".into(),
            snapshot_sha256: rule_fixtures::hash(snapshot.as_bytes()),
            faces,
        },
    )
    .unwrap();
    (catalog, references)
}

fn ward_program() -> Option<WellFormedCardProgram> {
    let mut ward = static_keywords::conditional_keyword();
    let Effect::Continuous {
        condition, patch, ..
    } = ward.node.declarations.first_mut()?
    else {
        return None;
    };
    *condition = None;
    *patch = CharacteristicPatch::GrantAbility {
        abilities: vec![GrantedAbility::Keyword {
            keyword: KeywordId::Ward,
        }],
    };
    Some(card(vec![ward]))
}

fn make_guard(actor: Viewer, secret: &str, catalog: &Arc<Catalog>) -> Game {
    let mut initial = board_fixtures::setup(actor.player().seat, json!([]));
    let (own, other) = if actor == Viewer::P1 {
        ("P1", "P2")
    } else {
        ("P2", "P1")
    };
    initial["turn"]["active"] = json!(own);
    initial["players"][own]["zones"]["field"] = json!([
        {"id":"physical-guard-0","card":"unit-follower"}, {"id":"physical-guard-1","card":"unit-follower"},
        {"id":"physical-acted","card":"unit-follower","state":{"acted":true}}, {"id":"physical-amulet","card":"unit-amulet"}
    ]);
    initial["players"][own]["zones"]["hand"] = json!(
        (0_u32..8)
            .map(|index| json!({"id":format!("physical-hand-{index}"),"card":"unit-amulet"}))
            .collect::<Vec<_>>()
    );
    initial["players"][other]["zones"]["field"] =
        json!([{"id":"physical-other-guard","card":"unit-follower"}]);
    initial["players"][other]["zones"]["hand"] = json!([{"id":"physical-secret","card":secret}]);
    initial["players"][other]["zones"]["deck"] = json!([
        {"id":"physical-next-0","card":"unit-spell"}, {"id":"physical-next-1","card":"unit-spell"}
    ]);
    let mut game = Game::new(
        Arc::clone(catalog),
        &initial,
        &Value::Null,
        &Value::Null,
        "wire-end-guard",
    )
    .unwrap();
    game.load_internal_card("unit-follower", &ward_program().unwrap())
        .unwrap();
    for number in ["unit-spell", "unit-amulet"] {
        game.load_internal_card(number, &card(vec![])).unwrap();
    }
    game.advance_player_input();
    let end = player_pipeline::end_command(&player_pipeline::question(&game, actor));
    assert_eq!(
        game.submit_player(actor, end).kind,
        SubmissionKind::Applied,
        "End enters the real GuardAct rule stop"
    );
    game
}

fn project_guard(game: &Game, actor: Viewer, references: &BattleReferences) -> QuestionProjection {
    game.battle_question(actor, &QuestionId::new("host-end-guard".into()), references)
        .unwrap()
        .unwrap()
}

const fn selection(projection: &QuestionProjection) -> Option<&wire::Selection> {
    if let wire::QuestionShape::Select { selection } = &projection.question().shape {
        Some(selection)
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
) -> Option<()> {
    let selection = selection(projection)?;
    assert_eq!(
        selection.candidates.len(),
        cards.len(),
        "the native eligible subset is complete"
    );
    assert_eq!(
        selection.bounds,
        wire::Bounds {
            min: Nat::from(0_u32),
            max: Some(Nat::from(2_u32))
        },
        "any subset is legal"
    );
    let field = &projection
        .observation()
        .players
        .iter()
        .find(|player| player.seat == seat(actor))?
        .zones
        .field;
    assert_eq!(
        field.count.get(),
        4,
        "the eligible domain is only part of the field"
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
        let member = field.cards.iter().find(|member| &member.object == handle)?;
        assert_eq!(
            handle.as_str(),
            native.handle.as_str(),
            "the candidate binds this physical guard"
        );
        assert_eq!(
            card, &object.effective_face,
            "the candidate shares the board face"
        );
        assert_eq!(
            candidate.controller,
            Some(object.controller),
            "the controller comes from this board"
        );
        assert_eq!(
            candidate.origin,
            Some(wire::Origin {
                player: seat(actor),
                zone: Zone::Field,
                position: member.position
            }),
            "the source position comes from this field member"
        );
        assert_eq!(
            object.state.as_ref()?.posture,
            Posture::Standing,
            "only standing guards are eligible"
        );
    }
    Some(())
}

fn assert_postures(game: &Game, actor: Viewer, references: &BattleReferences, count: usize) {
    let board = game.battle_observation(actor, references).unwrap();
    let field = &board
        .players
        .iter()
        .find(|player| player.seat == seat(actor))
        .unwrap()
        .zones
        .field;
    let postures = field
        .cards
        .iter()
        .map(|member| {
            board.objects[&member.object]
                .state
                .as_ref()
                .unwrap()
                .posture
        })
        .collect::<Vec<_>>();
    assert_eq!(
        postures,
        vec![
            if count == 2 {
                Posture::Acted
            } else {
                Posture::Standing
            },
            if count > 0 {
                Posture::Acted
            } else {
                Posture::Standing
            },
            Posture::Acted,
            Posture::Standing
        ],
        "only the selected eligible guards act; prior posture and the non-guard are preserved"
    );
}

#[test]
fn wire_end_guard_round_trips_empty_partial_and_complete_subsets() {
    let (catalog, references) = fixture();
    for actor in [Viewer::P1, Viewer::P2] {
        for count in 0_usize..=2 {
            let mut game = make_guard(actor, "unit-spell", &catalog);
            let native = player_pipeline::question(&game, actor);
            let QuestionShape::Closing(ClosingQuestion::GuardAct(cards)) = native.shape else {
                panic!("native GuardAct")
            };
            assert_eq!(cards.len(), 2);
            assert!(
                game.battle_observation(actor, &references).is_ok(),
                "the native rule board is independently projectable"
            );
            let before = exact_authority(&game);
            let projection = project_guard(&game, actor, &references);
            assert!(
                exact_authority(&game) == before,
                "projection cannot advance the closing rule"
            );
            assert_eq!(assert_candidates(&projection, &cards, actor), Some(()));
            assert_eq!(projection.question().usage, wire::QuestionUse::EndGuard);
            assert_eq!(
                projection.question().processing.stage,
                ProcessingStage::Closing
            );
            assert_eq!(
                projection.observation(),
                &game.battle_observation(actor, &references).unwrap()
            );
            let chosen = selection(&projection)
                .unwrap()
                .candidates
                .iter()
                .rev()
                .take(count)
                .map(|card| card.id.clone())
                .collect();
            let command = projection
                .answer(actor, &projection.question().id, &select(chosen))
                .unwrap();
            let Command::Closing {
                token,
                answer: ClosingAnswer::GuardAct(handles),
            } = &command
            else {
                panic!("native GuardAct answer")
            };
            assert_eq!(*token, native.token);
            assert_eq!(
                *handles,
                cards
                    .iter()
                    .rev()
                    .take(count)
                    .map(|card| card.handle.clone())
                    .collect::<Vec<_>>()
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
                "a stale GuardAct command preserves the new question"
            );
            assert_postures(&game, actor, &references, count);
        }
    }
}

#[test]
fn wire_end_guard_skipping_all_guards_continues_to_hand_limit() {
    let (catalog, references) = fixture();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut game = make_guard(actor, "unit-spell", &catalog);
        let projection = project_guard(&game, actor, &references);
        let before_token = player_pipeline::question(&game, actor).token;
        let command = projection
            .answer(actor, &projection.question().id, &select(vec![]))
            .unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        assert_postures(&game, actor, &references, 0);
        let quick = player_pipeline::question(&game, opposite(actor));
        assert!(matches!(
            quick.shape,
            QuestionShape::Decision(DecisionShape::Quick { .. })
        ));
        assert_eq!(
            game.submit_player(
                opposite(actor),
                Command::Answer {
                    token: quick.token,
                    answer: Answer::Quick(None)
                }
            )
            .kind,
            SubmissionKind::Applied
        );
        let native = player_pipeline::question(&game, actor);
        assert!(matches!(
            native.shape,
            QuestionShape::Closing(ClosingQuestion::Discard { count: 1, .. })
        ));
        assert_ne!(native.token, before_token);
        let discard = game
            .battle_question(
                actor,
                &QuestionId::new("host-after-guard".into()),
                &references,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            discard.question().usage,
            wire::QuestionUse::HandLimitDiscard
        );
        let first = selection(&discard)
            .unwrap()
            .candidates
            .first()
            .unwrap()
            .id
            .clone();
        let command = discard
            .answer(actor, &discard.question().id, &select(vec![first]))
            .unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
    }
}

#[test]
fn wire_end_guard_rejects_invalid_sets_and_wrong_bindings() {
    let (catalog, references) = fixture();
    let actor = Viewer::P1;
    let mut game = make_guard(actor, "unit-spell", &catalog);
    let projection = project_guard(&game, actor, &references);
    let first = selection(&projection)
        .unwrap()
        .candidates
        .first()
        .unwrap()
        .id
        .clone();
    let before = exact_authority(&game);
    for answer in [
        select(vec![first.clone(), first.clone()]),
        select(vec![OptionId::new("non-guard".into())]),
        WireAnswer::Select {
            selected: vec![],
            groups: Some(vec![]),
            roles: vec![],
        },
        WireAnswer::Select {
            selected: vec![first.clone()],
            groups: None,
            roles: vec![RoleChoice {
                role: Nat::from(1_u32),
                selected: vec![first.clone()],
            }],
        },
        WireAnswer::Choose {
            chosen: vec![first.clone()],
        },
    ] {
        assert!(matches!(
            projection.answer(actor, &projection.question().id, &answer),
            Err(AnswerError)
        ));
    }
    let valid = select(vec![first]);
    assert!(matches!(
        projection.answer(Viewer::P2, &projection.question().id, &valid),
        Err(AnswerError)
    ));
    assert!(matches!(
        projection.answer(actor, &QuestionId::new("another".into()), &valid),
        Err(AnswerError)
    ));
    let another = game
        .battle_question(actor, &QuestionId::new("another".into()), &references)
        .unwrap()
        .unwrap();
    assert!(matches!(
        another.answer(actor, &another.question().id, &valid),
        Err(AnswerError)
    ));
    let command = projection
        .answer(actor, &projection.question().id, &valid)
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P2, command).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == before,
        "invalid bindings and native wrong actor leave the authority intact"
    );
}

#[test]
fn wire_end_guard_question_and_board_ignore_the_other_hand() {
    let (catalog, references) = fixture();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut views = Vec::new();
        for secret in ["unit-spell", "unit-amulet"] {
            let game = make_guard(actor, secret, &catalog);
            let projection = project_guard(&game, actor, &references);
            assert!(
                game.battle_question(
                    opposite(actor),
                    &QuestionId::new("waiting".into()),
                    &references
                )
                .unwrap()
                .is_none()
            );
            let serialized =
                serde_json::to_string(&(projection.observation(), projection.question())).unwrap();
            for private in ["physical-", "token", "serial", "epoch"] {
                assert!(!serialized.contains(private));
            }
            assert!(!format!("{projection:?}").contains("vobj:"));
            views.push(projection);
        }
        assert_eq!(views[0].observation(), views[1].observation());
        assert_eq!(views[0].question(), views[1].question());
    }
}
