use super::*;
use std::fs::read_to_string;
use std::path::PathBuf;

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
        vec![granted(KeywordId::Drain, DurationEnd::NoTemporalExpiry {})],
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

fn grant_position(steps: Vec<Located<Node>>) -> Game {
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
    let mut game = Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "keyword-grant",
    )
    .unwrap();
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
