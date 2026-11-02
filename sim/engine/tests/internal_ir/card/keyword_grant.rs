use super::*;
use std::fs::read_to_string;
use std::path::PathBuf;
use sve_engine::game::internal::InputDomain;

fn granted(keyword: KeywordId, end: DurationEnd) -> Located<Node> {
    instruction(
        1,
        Action::Grant {
            subjects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            abilities: vec![GrantedAbility::Keyword { keyword }],
            duration: Duration {
                end,
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            provider: TextProvider {
                provider: ObjectView::Current {
                    object: Box::new(ObjectOperand::SourceHolder {}),
                },
                face: FaceId("0".into()),
                ability: AbilityId(1),
            },
        },
    )
}

#[test]
fn basic_keyword_grants_are_effective_and_other_keywords_stay_closed() {
    for (keyword, key) in [
        (KeywordId::Ward, "守護"),
        (KeywordId::Aura, "オーラ"),
        (KeywordId::Storm, "疾走"),
        (KeywordId::Rush, "突進"),
        (KeywordId::Bane, "必殺"),
        (KeywordId::Intimidate, "威圧"),
        (KeywordId::Assail, "指定攻撃"),
    ] {
        let mut game = grant_position(vec![granted(keyword, DurationEnd::NoTemporalExpiry {})]);
        let result = activate(&mut game, 1);
        assert!(result.progress.receipts.values().any(|receipt| matches!(receipt, InstructionOutcome::Granted(subjects) if subjects.len() == 1)));
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
            json!([key])
        );
    }
    let bad = ability(
        1,
        vec![granted(
            KeywordId::TwinDrive,
            DurationEnd::NoTemporalExpiry {},
        )],
    );
    let error = WellFormedCardProgram::check(CardProgram {
        abilities: vec![bad],
        card_play_costs: vec![],
    })
    .unwrap_err();
    assert_eq!(error.reason, WellFormedReason::UnsupportedShape);
}

#[test]
fn keyword_grant_expires_at_the_actual_turn_boundary() {
    let mut game = grant_position(vec![granted(
        KeywordId::Rush,
        DurationEnd::CurrentTurnEnd {},
    )]);
    activate(&mut game, 1);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
        json!(["突進"])
    );
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    quick::pass(&mut game);
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["turn"]["active"], "P2");
    assert_eq!(state["objects"]["s"]["keywords"], json!([]));
}

fn grant_game() -> Game {
    let (snapshot, _, document) = catalog_documents(&json!("1"));
    let registry = read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../authored/rules/keywords.yaml"),
    )
    .unwrap();
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &registry, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut initial = setup(1, json!([{"id":"d","card":"unit-spell"}]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"s","card":"unit-follower"}]);
    Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "keyword-grant",
    )
    .unwrap()
}

fn grant_position(steps: Vec<Located<Node>>) -> Game {
    let mut game = grant_game();
    game.load_internal_card("unit-follower", &card(vec![ability(1, steps)]))
        .unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn keyword_grant_does_not_follow_a_recipient_into_a_new_field_generation() {
    let mut game = grant_position(vec![
        granted(KeywordId::Rush, DurationEnd::NoTemporalExpiry {}),
        movement::put(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Hand,
        ),
    ]);
    let result = activate(&mut game, 1);
    assert!(result.progress.receipts.values().any(
        |receipt| matches!(receipt, InstructionOutcome::Granted(subjects) if subjects.len() == 1)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["s"])
    );
    game.resume_internal_card().unwrap();
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["keywords"],
        json!([])
    );
}

#[test]
fn public_filter_grant_does_not_authorize_hidden_subjects() {
    for hidden_zone in [ZoneKind::Hand, ZoneKind::Deck] {
        let mut step = granted(KeywordId::Rush, DurationEnd::NoTemporalExpiry {});
        if let Node::Instruction { instruction } = &mut step.node
            && let Action::Grant { subjects, .. } = &mut instruction.action
        {
            *subjects = zone(hidden_zone, 1);
        }
        let program = card(vec![ability(1, vec![step])]);
        let mut game = grant_game();
        let before = game.projection(View::Referee).unwrap();
        let error = game
            .load_internal_card("unit-follower", &program)
            .unwrap_err();
        assert!(
            matches!(error, sve_engine::EngineFailure::Unsupported(reason)
            if reason.contains("private observation outside an authorized selector spine"))
        );
        assert_eq!(game.projection(View::Referee).unwrap(), before);
    }
}

#[test]
fn public_filter_grant_does_not_authorize_a_different_object_read() {
    let mut step = granted(KeywordId::Rush, DurationEnd::NoTemporalExpiry {});
    if let Node::Instruction { instruction } = &mut step.node
        && let Action::Grant { subjects, .. } = &mut instruction.action
    {
        *subjects = zone(ZoneKind::Field, 1);
        if let Selector::Zone { predicate, .. } = subjects {
            **predicate = Expr::Compute(Box::new(Predicate::ClassEquals {
                left: Expr::Live(Box::new(ClassRead::Class {
                    object: ObjectView::Current {
                        object: Box::new(ObjectOperand::SourceHolder {}),
                    },
                    view: CharacteristicView::Effective,
                })),
                right: Expr::Literal(Info::Present(ClassId("ニュートラル".into()))),
            }));
        }
    }
    let program = card(vec![ability(1, vec![step])]);
    let mut game = grant_game();
    let before = game.projection(View::Referee).unwrap();
    let error = game
        .load_internal_card("unit-follower", &program)
        .unwrap_err();
    assert!(
        matches!(error, sve_engine::EngineFailure::Unsupported(reason)
        if reason.contains("private observation outside an authorized selector spine"))
    );
    assert_eq!(game.projection(View::Referee).unwrap(), before);
}

fn trait_step(end: DurationEnd) -> Located<Node> {
    instruction(
        1,
        Action::Modify {
            subjects: Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            patch: CharacteristicPatch::AddTrait {
                trait_id: TraitId("武装".into()),
            },
            expires: Duration {
                end,
                anchor: TimeAnchor::Installation {},
                usage: None,
                intervals: vec![],
            },
            source: EffectReference::CurrentInstallation {},
        },
    )
}

#[test]
fn trait_addition_is_idempotent_and_survives_at_the_actual_turn_boundary() {
    let mut repeated = trait_step(DurationEnd::NoTemporalExpiry {});
    if let Node::Instruction { instruction } = &mut repeated.node {
        instruction.id = InstructionId(2);
        instruction.receipt = ReceiptId(2);
    }
    let mut game = grant_position(vec![trait_step(DurationEnd::NoTemporalExpiry {}), repeated]);
    activate(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["s"]["traits"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|value| **value == json!("武装"))
            .count(),
        1
    );
    game.resume_internal_card().unwrap();
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    assert!(game.internal_pending_choice().unwrap().is_none());
    quick::pass(&mut game);
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["s"]["traits"]
            .as_array()
            .unwrap()
            .contains(&json!("武装"))
    );
}

#[test]
fn trait_addition_does_not_follow_a_new_field_generation() {
    let mut game = grant_position(vec![
        trait_step(DurationEnd::NoTemporalExpiry {}),
        movement::put(
            2,
            Selector::Objects {
                objects: vec![ObjectOperand::SourceHolder {}],
            },
            OwnerZone::Hand,
        ),
    ]);
    activate(&mut game, 1);
    game.resume_internal_card().unwrap();
    assert!(matches!(
        game.play_internal_card("s", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    assert!(
        !game.projection(View::Referee).unwrap()["objects"]["s"]["traits"]
            .as_array()
            .unwrap()
            .contains(&json!("武装"))
    );
}

#[test]
fn added_traits_feed_the_same_effects_public_filters_and_empty_sets_are_harmless() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = json!({"cost":{"pp":0},"do":[
        {"apply":{"on":"this","effect":{"add_trait":{"trait":"武装"}}}},
        {"buff":{"on":{"who":"self","zone":"field","trait":"武装"},"hp":1}},
        {"apply":{"on":{"who":"self","zone":"field","trait":"absent"},"effect":{"add_trait":{"trait":"unused"}}}}
    ]});
    tree["cards"] = json!({"unit-follower":seed});
    let program = lowerer().lower(&tree).unwrap()["unit-follower"].clone();
    let mut game = grant_game();
    game.load_internal_card("unit-follower", &program).unwrap();
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    let before = game.projection(View::Referee).unwrap()["objects"]["s"]["hp"]
        .as_i64()
        .unwrap();
    let result = activate(&mut game, 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["s"]["hp"], before + 1);
    assert!(
        !state["objects"]["s"]["traits"]
            .as_array()
            .unwrap()
            .contains(&json!("unused"))
    );
    assert!(result.progress.receipts.values().any(|outcome| matches!(outcome, InstructionOutcome::Modified { effect: None, subjects } if subjects.is_empty())));
}

#[test]
fn trait_grants_keep_remove_and_unimplemented_durations_closed() {
    for patch in [
        CharacteristicPatch::AddTrait {
            trait_id: TraitId(String::new()),
        },
        CharacteristicPatch::RemoveTrait {
            trait_id: TraitId("武装".into()),
        },
    ] {
        let mut step = trait_step(DurationEnd::NoTemporalExpiry {});
        if let Node::Instruction { instruction } = &mut step.node
            && let Action::Modify { patch: current, .. } = &mut instruction.action
        {
            *current = patch;
        }
        WellFormedCardProgram::check(CardProgram {
            abilities: vec![ability(1, vec![step])],
            card_play_costs: vec![],
        })
        .unwrap_err();
    }
    let program = card(vec![ability(
        1,
        vec![trait_step(DurationEnd::CurrentTurnEnd {})],
    )]);
    let mut game = grant_game();
    let before = exact_authority(&game);
    assert!(game.load_internal_card("unit-follower", &program).is_err());
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn trait_lowering_rejects_an_inner_subject_instead_of_ignoring_it() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut seed = tree["cards"]["BP01-041"].clone();
    seed["abilities"][0]["activate"] = json!({"cost":{"pp":0},"do":[
        {"apply":{"on":"this","effect":{"add_trait":{"trait":"武装","on":"affected"}}}}
    ]});
    tree["cards"] = json!({"unit-follower":seed});
    lowerer().lower(&tree).unwrap_err();
}

#[test]
fn departure_triggers_read_the_added_trait_from_the_same_lki_as_history() {
    let mut tree = fanfare::expanded::tree().clone();
    let mut source = tree["cards"]["BP01-041"].clone();
    source["abilities"][0]["activate"] = json!({"cost":{"pp":0},"do":[
        {"apply":{"on":"this","effect":{"add_trait":{"trait":"武装"}}}},
        {"move":{"ref":"this","to":"cemetery"}}
    ]});
    let mut observer = tree["cards"]["BP01-041"].clone();
    observer["abilities"][0]
        .as_object_mut()
        .unwrap()
        .remove("activate");
    observer["abilities"][0]["auto"] = json!({
        "on":{"moved":{"from":{"who":"self","zone":"field"},"to":"cemetery","sel":{"kind":"follower","trait":"武装"}}},
        "do":[{"life":{"who":"self","delta":1}}]
    });
    tree["cards"] = json!({"unit-follower":source,"unit-amulet":observer});
    let programs = lowerer().lower(&tree).unwrap();
    let mut initial = setup(1, json!([]));
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"s","card":"unit-follower"},{"id":"observer","card":"unit-amulet"}
    ]);
    let mut game = Game::new(catalog(), &initial, &Value::Null, &Value::Null, "trait-lki").unwrap();
    for number in ["unit-follower", "unit-amulet"] {
        game.load_internal_card(number, &programs[number]).unwrap();
    }
    game.load_internal_card("unit-spell", &card(vec![]))
        .unwrap();
    activate(&mut game, 1);
    game.resume_internal_card().unwrap();
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("departure trigger");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "unit-amulet");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        21
    );
}
