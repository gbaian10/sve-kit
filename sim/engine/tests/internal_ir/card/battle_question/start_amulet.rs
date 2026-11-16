use super::*;
use crate::{board_fixtures, rule_fixtures};
use sve_battle_contract::{CardRef, Nat, RoleChoice, observation::Facing};
use sve_engine::battle::{BattleReferences, PrintingFaceIdentity, ReferenceInput};
use sve_engine::catalog::EngineIdentityInput;

fn fixture() -> (Arc<Catalog>, BattleReferences) {
    let (base_snapshot, keywords, document) = board_fixtures::catalog_documents(&json!("1"));
    let mut rows: Vec<Value> = base_snapshot
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for row in &mut rows {
        if row["number"] == "unit-follower" {
            row["faces"][0]["title"] = json!("synthetic-start-label");
        }
    }
    let snapshot = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let mut document: Value = serde_json::from_str(&document).unwrap();
    document["cards"]["unit-follower"]["abilities"] = json!([
        {"kind":"static","line":1_i32,"body":{"op":"keyword","name":"start_amulet"}}
    ]);
    let mut keywords: Value = serde_json::from_str(&keywords).unwrap();
    keywords["keywords"]["start_amulet"] = json!({"ja":"スタートアミュレット","rule":"14.4.3",
        "expansion":{"op":"keyword","name":"start_amulet"}});
    let rules = rule_fixtures::rules(
        &snapshot,
        json!([rule_fixtures::title(
            "synthetic_start",
            "unit-follower",
            json!([{"kind":"opening_start_amulet"}])
        )]),
        json!([]),
    );
    let catalog = Arc::new(
        Catalog::from_documents_with_rules(
            &snapshot,
            &keywords.to_string(),
            &[("synthetic-start".into(), document.to_string())],
            &rules.to_string(),
            &EngineIdentityInput::LegacyJp,
        )
        .unwrap(),
    );
    let faces = ["unit-follower", "unit-spell", "unit-amulet"]
        .into_iter()
        .map(|number| PrintingFaceIdentity {
            card_no: number.into(),
            native_face: 0,
            card: CardRef {
                printing_id: format!("printing-{number}"),
                face: Nat::from(0_u32),
            },
        })
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

fn make_start(catalog: &Arc<Catalog>, fillers: [&str; 2]) -> Game {
    let mut players = json!({});
    for (player, filler) in ["P1", "P2"].into_iter().zip(fillers) {
        let mut deck = (0_u32..8)
            .map(|index| json!({"id":format!("physical-{player}-{index}"),"card":filler}))
            .collect::<Vec<_>>();
        for index in 0_u32..2 {
            deck.push(
                json!({"id":format!("physical-{player}-start-{index}"),"card":"unit-follower"}),
            );
        }
        players[player] = json!({"construction":"title","title_code":"synthetic_start",
            "leader":{"class":"ニュートラル"},"deck_list":deck,"evolve_deck_list":[]});
    }
    Game::new(
        Arc::clone(catalog),
        &json!({"pregame":true,"players":players}),
        &Value::Null,
        &json!({"first_chooser":"P2"}),
        "wire-start",
    )
    .unwrap()
}

fn projected(
    game: &Game,
    actor: Viewer,
    references: &BattleReferences,
    id: &str,
) -> QuestionProjection {
    game.battle_question(actor, &QuestionId::new(id.into()), references)
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

fn select(id: &OptionId) -> WireAnswer {
    WireAnswer::Select {
        selected: vec![id.clone()],
        groups: None,
        roles: vec![],
    }
}

fn choose_last(game: &mut Game, actor: Viewer, references: &BattleReferences) {
    let projection = projected(game, actor, references, &format!("host-start-{actor:?}"));
    let candidates = &selection(&projection).unwrap().candidates;
    let command = projection
        .answer(
            actor,
            &projection.question().id,
            &select(&candidates.last().unwrap().id),
        )
        .unwrap();
    assert_eq!(
        game.submit_player(actor, command).kind,
        SubmissionKind::Applied,
        "the projected start-amulet choice must advance the native opening"
    );
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
        "native and wire candidate lengths agree"
    );
    assert_eq!(
        selection.bounds,
        wire::Bounds {
            min: Nat::from(1_u32),
            max: Some(Nat::from(1_u32))
        },
        "start amulets require exactly one eligible object"
    );
    assert!(
        selection.groups.is_empty() && selection.roles.is_empty() && selection.distinct.is_empty(),
        "start amulets impose no group, role or distinct constraints"
    );
    let deck = &projection
        .observation()
        .players
        .iter()
        .find(|player| player.seat == seat(actor))?
        .zones
        .deck;
    assert_eq!(
        deck.count.get(),
        10,
        "eligible cards are a subset of the deck"
    );
    for (candidate, native) in selection.candidates.iter().zip(cards) {
        let Some(Subject::Card {
            card,
            object: Some(handle),
        }) = &candidate.subject
        else {
            return None;
        };
        assert_eq!(
            handle.as_str(),
            native.handle.as_str(),
            "the native capability identifies this candidate"
        );
        let object = projection.observation().objects.get(handle)?;
        assert_eq!(
            card, &object.effective_face,
            "the candidate uses the same board face"
        );
        assert_eq!(
            candidate.controller,
            Some(object.controller),
            "the controller comes from this object table"
        );
        let member = deck.cards.iter().find(|member| &member.object == handle)?;
        assert_eq!(
            candidate.origin,
            Some(wire::Origin {
                player: seat(actor),
                zone: Zone::Deck,
                position: member.position
            }),
            "the origin preserves this board member's authorized position"
        );
    }
    Some(())
}

#[test]
fn wire_start_amulet_selects_the_second_same_printing_for_each_seat() {
    let (catalog, references) = fixture();
    let mut game = make_start(&catalog, ["unit-spell", "unit-amulet"]);
    for actor in [Viewer::P1, Viewer::P2] {
        let Input::Question(native) = game.input(actor) else {
            panic!("native start question")
        };
        let QuestionShape::Opening(OpeningQuestion::StartAmulet(cards)) = native.shape else {
            panic!("native start shape")
        };
        assert_eq!(cards.len(), 2);
        assert!(
            game.battle_observation(actor, &references).is_ok(),
            "the native board is independently projectable"
        );
        let before = exact_authority(&game);
        let projection = projected(&game, actor, &references, &format!("host-start-{actor:?}"));
        assert!(
            exact_authority(&game) == before,
            "projection cannot advance the opening"
        );
        assert_eq!(assert_candidates(&projection, &cards, actor), Some(()));
        assert_eq!(
            projection.observation(),
            &game.battle_observation(actor, &references).unwrap()
        );
        assert_eq!(projection.question().usage, wire::QuestionUse::StartAmulet);
        assert!(projection.question().is_coherent());
        assert!(
            game.battle_question(
                opposite(actor),
                &QuestionId::new("waiting".into()),
                &references
            )
            .unwrap()
            .is_none()
        );
        let candidates = &selection(&projection).unwrap().candidates;
        let (
            Some(Subject::Card {
                card: first,
                object: first_object,
            }),
            Some(Subject::Card {
                card: second,
                object: second_object,
            }),
        ) = (&candidates[0].subject, &candidates[1].subject)
        else {
            panic!("card subjects")
        };
        assert_eq!(first, second);
        assert_ne!(first_object, second_object);
        let command = projection
            .answer(actor, &projection.question().id, &select(&candidates[1].id))
            .unwrap();
        let Command::Opening {
            token,
            answer: OpeningAnswer::StartAmulet(chosen),
        } = &command
        else {
            panic!("native start answer")
        };
        assert_eq!(*token, native.token);
        assert_eq!(*chosen, cards[1].handle);
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
        assert_field(&game, &references, actor, false);
    }
}

fn assert_field(game: &Game, references: &BattleReferences, owner: Viewer, public: bool) {
    for viewer in [owner, opposite(owner)] {
        let board = game.battle_observation(viewer, references).unwrap();
        let field = &board
            .players
            .iter()
            .find(|player| player.seat == seat(owner))
            .unwrap()
            .zones
            .field;
        assert_eq!(field.count.get(), 1, "the selected object is on the field");
        if viewer == owner || public {
            assert_eq!(
                field.cards.len(),
                1,
                "the viewer may identify this field card"
            );
            let object = &board.objects[&field.cards[0].object];
            assert_eq!(
                object.facing,
                if public { Facing::Up } else { Facing::Down },
                "both opening mulligans must finish before the selected face is public"
            );
        } else {
            assert!(
                field.cards.is_empty(),
                "the other seat cannot identify the facedown start amulet"
            );
        }
    }
}

#[test]
fn wire_start_amulet_forgets_unselected_handles_and_reveals_after_both_mulligans() {
    let (catalog, references) = fixture();
    let mut game = make_start(&catalog, ["unit-spell", "unit-amulet"]);
    let mut leftovers = Vec::new();
    for actor in [Viewer::P1, Viewer::P2] {
        let projection = projected(&game, actor, &references, &format!("host-start-{actor:?}"));
        let Some(Subject::Card {
            object: Some(handle),
            ..
        }) = &selection(&projection).unwrap().candidates[0].subject
        else {
            panic!("card subject")
        };
        leftovers.push((actor, handle.clone()));
        choose_last(&mut game, actor, &references);
    }
    for (actor, old_handle) in leftovers {
        assert!(
            !game
                .battle_observation(actor, &references)
                .unwrap()
                .objects
                .contains_key(&old_handle),
            "shuffling the unselected deck card must retire its former capability"
        );
    }
    let first = projected(&game, Viewer::P2, &references, "host-first");
    let command = first
        .answer(
            Viewer::P2,
            &first.question().id,
            &first_answer(&first, Viewer::P1).unwrap(),
        )
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P2, command).kind,
        SubmissionKind::Applied
    );
    for actor in [Viewer::P1, Viewer::P2] {
        for owner in [Viewer::P1, Viewer::P2] {
            assert_field(&game, &references, owner, false);
        }
        let projection = projected(&game, actor, &references, &format!("host-hand-{actor:?}"));
        assert_eq!(projection.question().usage, wire::QuestionUse::Mulligan);
        let keep = WireAnswer::Mulligan {
            redraw: false,
            bottom_order: vec![],
        };
        let command = projection
            .answer(actor, &projection.question().id, &keep)
            .unwrap();
        assert_eq!(
            game.submit_player(actor, command).kind,
            SubmissionKind::Applied
        );
    }
    for owner in [Viewer::P1, Viewer::P2] {
        assert_field(&game, &references, owner, true);
    }
}

#[test]
fn wire_start_amulet_rejects_invalid_selections_without_advancing_authority() {
    let (catalog, references) = fixture();
    let mut game = make_start(&catalog, ["unit-spell", "unit-amulet"]);
    let projection = projected(&game, Viewer::P1, &references, "host-start-P1");
    let candidates = &selection(&projection).unwrap().candidates;
    let chosen = candidates[0].id.clone();
    let before = exact_authority(&game);
    for answer in [
        WireAnswer::Select {
            selected: vec![],
            groups: None,
            roles: vec![],
        },
        WireAnswer::Select {
            selected: vec![chosen.clone(), candidates[1].id.clone()],
            groups: None,
            roles: vec![],
        },
        WireAnswer::Select {
            selected: vec![chosen.clone(), chosen.clone()],
            groups: None,
            roles: vec![],
        },
        select(&OptionId::new("foreign".into())),
        WireAnswer::Select {
            selected: vec![chosen.clone()],
            groups: Some(vec![]),
            roles: vec![],
        },
        WireAnswer::Select {
            selected: vec![chosen.clone()],
            groups: None,
            roles: vec![RoleChoice {
                role: Nat::from(1_u32),
                selected: vec![chosen.clone()],
            }],
        },
        WireAnswer::Choose {
            chosen: vec![chosen.clone()],
        },
    ] {
        assert!(matches!(
            projection.answer(Viewer::P1, &projection.question().id, &answer),
            Err(AnswerError)
        ));
    }
    let valid = select(&chosen);
    assert!(matches!(
        projection.answer(Viewer::P2, &projection.question().id, &valid),
        Err(AnswerError)
    ));
    assert!(matches!(
        projection.answer(
            Viewer::P1,
            &QuestionId::new("another-question".into()),
            &valid
        ),
        Err(AnswerError)
    ));
    assert!(
        exact_authority(&game) == before,
        "invalid wire answers leave the exact current authority intact"
    );
    let command = projection
        .answer(Viewer::P1, &projection.question().id, &valid)
        .unwrap();
    assert_eq!(
        game.submit_player(Viewer::P2, command.clone()).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == before,
        "native submission still checks the authenticated actor"
    );
    assert_eq!(
        game.submit_player(Viewer::P1, command.clone()).kind,
        SubmissionKind::Applied
    );
    let after = exact_authority(&game);
    assert_eq!(
        game.submit_player(Viewer::P1, command).kind,
        SubmissionKind::Rejected
    );
    assert!(
        exact_authority(&game) == after,
        "the native token rejects a retired start question"
    );
}

#[test]
fn wire_start_amulet_board_and_question_do_not_depend_on_the_other_deck() {
    let (catalog, references) = fixture();
    for actor in [Viewer::P1, Viewer::P2] {
        let mut projections = Vec::new();
        for secret in ["unit-spell", "unit-amulet"] {
            let fillers = if actor == Viewer::P1 {
                ["unit-spell", secret]
            } else {
                [secret, "unit-spell"]
            };
            let mut game = make_start(&catalog, fillers);
            if actor == Viewer::P2 {
                choose_last(&mut game, Viewer::P1, &references);
            }
            let projection = projected(&game, actor, &references, &format!("host-start-{actor:?}"));
            let serialized =
                serde_json::to_string(&(projection.observation(), projection.question())).unwrap();
            for private in ["physical-", "token", "serial", "epoch"] {
                assert!(!serialized.contains(private));
            }
            assert!(!format!("{projection:?}").contains("vobj:"));
            projections.push(projection);
        }
        assert_eq!(projections[0].observation(), projections[1].observation());
        assert_eq!(projections[0].question(), projections[1].question());
    }
}
