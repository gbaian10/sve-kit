use super::*;

#[test]
fn construction_only_cards_load_without_creating_game_abilities() {
    let selected = selection();
    for number in ["CP01-085", "CP03-127", "ECP01-058"] {
        let card = &selected.current[number];
        assert_eq!(card.program.card().abilities.len(), 0);
        assert_eq!(
            card.expanded["cards"][number]["deck_rule"],
            json!({"deck":"evolve", "same_name_max":10_u8})
        );
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!((0_u8..10)
            .map(|n| json!({"id":format!("copy{n}"), "card":number}))
            .collect::<Vec<_>>());
        let game = game(&initial);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["evolve_deck"]
                .as_array()
                .unwrap()
                .len(),
            10
        );
    }
}

#[test]
fn construction_metadata_must_match_the_complete_current_handoff() {
    let input = inputs().2["authors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|author| author["input"]["card_no"] == "CP03-127")
        .unwrap()["input"]
        .clone();
    for mutation in [
        "missing", "limit", "zone", "owner", "line", "overlap", "invented",
    ] {
        let mut changed = input.clone();
        match mutation {
            "missing" => {
                changed["tree"]["cards"]["CP03-127"]
                    .as_object_mut()
                    .unwrap()
                    .remove("deck_rule");
            }
            "limit" => {
                changed["tree"]["cards"]["CP03-127"]["deck_rule"]["same_name_max"] = json!(9_u8);
            }
            "zone" => changed["tree"]["cards"]["CP03-127"]["deck_rule"]["deck"] = json!("main"),
            "owner" => {
                changed["source_payload"]["deck_rules"][0]["source"]["owner"]["revision_id"] =
                    json!("other");
            }
            "line" => changed["source_payload"]["deck_rules"][0]["source"]["line"] = json!(99_u8),
            "overlap" => {
                let source = changed["source_payload"]["deck_rules"][0]["source"].clone();
                changed["tree"]["cards"]["CP03-127"]["ignored"]
                    .as_array_mut()
                    .unwrap()
                    .push(source);
            }
            _ => changed["source_payload"]["deck_rules"] = json!([]),
        }
        assert!(lowerer().lower_current(&changed).is_err(), "{mutation}");
    }
}
