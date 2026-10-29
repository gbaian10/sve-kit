use super::*;
use sve_engine::game::internal::InputDomain;

fn advance_board(zone: &str, kind: &str, cost: &str, active: &str) -> Game {
    let (snapshot, keywords, document) = catalog_documents(&json!(cost));
    let snapshot = snapshot
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row["number"] == "unit-spell" {
                row["faces"][0]["card_type"] = json!(kind);
                row["faces"][0]["power"] = json!("-");
                row["faces"][0]["hp"] = json!("-");
            }
            row.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let catalog = Arc::new(
        Catalog::from_documents(&snapshot, &keywords, &[("unit.yaml".into(), document)]).unwrap(),
    );
    let mut initial = setup(1, json!([{"id":"d","card":"unit-follower"}]));
    initial["turn"]["active"] = json!(active);
    initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"s","card":"unit-spell"}]);
    Game::new(
        catalog,
        &initial,
        &Value::Null,
        &Value::Null,
        "spell-advance",
    )
    .unwrap()
}

fn load_advance(game: &mut Game, program: &WellFormedCardProgram) {
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game.load_internal_card("unit-spell", program).unwrap();
}

#[test]
fn spell_advance_pays_resolves_and_returns_face_up_to_its_owner() {
    let program = card(vec![play::spell(vec![draw(1, 1)])]);
    let mut game = advance_board("ex", "スペル・アドバンス", "1", "P1");
    load_advance(&mut game, &program);
    let result = resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 3);
    assert_eq!(state["P1"]["hand"], json!(["d"]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert_eq!(state["P1"]["evolve_deck"], json!(["s"]));
    assert_eq!(state["objects"]["s"]["generation"], 3_u8);
    assert_eq!(state["objects"]["s"]["face_up"], true);
    assert!(
        result
            .progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::CardResolved { .. }))
    );
    game.resume_internal_card().unwrap();
    let before = exact_authority(&game);
    game.play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(exact_authority(&game), before);
}

#[test]
fn spell_advance_rejects_wrong_cost_kind_zone_and_controller_atomically() {
    let program = card(vec![play::spell(vec![draw(1, 1)])]);
    for (kind, cost) in [("スペル・アドバンス", "2"), ("スペル・エボルヴ", "1")] {
        let mut game = advance_board("ex", kind, cost, "P1");
        let before = exact_authority(&game);
        game.load_internal_card("unit-spell", &program).unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
    for (zone, seat) in [("evolve_deck", 1), ("cemetery", 1), ("ex", 2)] {
        let mut game = advance_board(zone, "スペル・アドバンス", "1", "P1");
        load_advance(&mut game, &program);
        let before = exact_authority(&game);
        game.play_internal_card("s", PlayerId { seat }).unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}

#[test]
fn spell_advance_declared_quick_uses_its_authority_window() {
    let program = card(vec![play::spell(vec![draw(1, 1)]), quick::permit()]);
    let mut game = advance_board("ex", "スペル・アドバンス", "1", "P2");
    load_advance(&mut game, &program);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let question = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &question.domain else {
        panic!("Quick authority window");
    };
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "unit-spell");
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 3);
    assert_eq!(state["P1"]["hand"], json!(["d"]));
    assert_eq!(state["P1"]["evolve_deck"], json!(["s"]));
}

#[test]
fn spell_advance_return_generation_exhaustion_rolls_back_the_whole_play() {
    let program = card(vec![play::spell(vec![draw(1, 1)])]);
    let original = advance_board("ex", "スペル・アドバンス", "1", "P1");
    let mut saved = serde_json::to_value(&original).unwrap();
    saved["state"]["objects"]["s"]["generation"] = json!(u64::MAX - 2);
    let mut game: Game = serde_json::from_value(saved).unwrap();
    load_advance(&mut game, &program);
    let before = exact_authority(&game);
    let error = game
        .play_internal_card("s", PlayerId { seat: 1 })
        .unwrap_err();
    assert!(error.to_string().contains("generation"), "{error}");
    assert_eq!(exact_authority(&game), before);
}
