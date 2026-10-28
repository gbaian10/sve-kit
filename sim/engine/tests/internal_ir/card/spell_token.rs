use super::*;

fn token_board(zone: &str, kind: &str, cost: &str) -> Game {
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
    initial["players"]["P1"]["pp"] = json!({"current":4,"max":4});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"][zone] = json!([{"id":"s","card":"unit-spell"}]);
    let mut game = Game::new(catalog, &initial, &Value::Null, &Value::Null, "spell-token").unwrap();
    game.load_internal_card("unit-follower", &card(vec![]))
        .unwrap();
    game
}

#[test]
fn spell_token_uses_formal_payment_resolution_and_erasure_from_legal_zones() {
    let program = card(vec![play::spell(vec![draw(1, 1)])]);
    for zone in ["hand", "ex"] {
        let mut game = token_board(zone, "スペル・トークン", "1");
        game.load_internal_card("unit-spell", &program).unwrap();
        let result = resolved(game.play_internal_card("s", PlayerId { seat: 1 }).unwrap());
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 3);
        assert_eq!(view["P1"]["hand"], json!(["d"]));
        assert_eq!(view["P1"]["cemetery"], json!([]));
        assert_eq!(view["P1"]["ex"], json!([]));
        assert!(view["objects"]["s"].is_null());
        assert!(
            result
                .progress
                .facts
                .iter()
                .any(|fact| matches!(fact, Fact::CardResolved { .. }))
        );
        game.resume_internal_card().unwrap();
        game.play_internal_card("s", PlayerId { seat: 1 })
            .unwrap_err();
    }
}

#[test]
fn spell_token_rejects_wrong_kind_cost_zone_and_controller_atomically() {
    let program = card(vec![play::spell(vec![draw(1, 1)])]);
    for (kind, cost) in [("フォロワー・トークン", "1"), ("スペル・トークン", "0")]
    {
        let mut game = token_board("ex", kind, cost);
        let before = exact_authority(&game);
        game.load_internal_card("unit-spell", &program).unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
    for (zone, seat) in [("deck", 1), ("cemetery", 1), ("ex", 2)] {
        let mut game = token_board(zone, "スペル・トークン", "1");
        game.load_internal_card("unit-spell", &program).unwrap();
        let before = exact_authority(&game);
        game.play_internal_card("s", PlayerId { seat }).unwrap_err();
        assert_eq!(exact_authority(&game), before);
    }
}
