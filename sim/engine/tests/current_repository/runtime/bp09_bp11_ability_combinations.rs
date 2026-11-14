use super::ability_combinations::*;
use super::*;

#[test]
fn bp10_004_acts_to_damage_an_entering_enemy_follower() {
    let mut initial = position("BP10-004", "field");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"acted":false});
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"goblin","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    let step = game
        .play_internal_card("goblin", PlayerId { seat: 2 })
        .unwrap();
    finish(&mut game, Some(step), &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_eq!(state["P2"]["cemetery"], json!(["goblin"]));
    assert_eq!(state["P2"]["pp"]["current"], 9_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp10_029_resolves_the_token_fanfare_before_the_paid_ward_buff() {
    let initial = position("BP10-029", "hand");
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    let followers = state["P1"]["field"].as_array().unwrap();
    assert_eq!(followers.len(), 3);
    for id in followers.iter().filter(|id| **id != json!("source")) {
        let object = &state["objects"][id.as_str().unwrap()];
        assert_eq!(object["power"], 2_u8);
        assert_eq!(object["hp"], 3_u8);
    }
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
}

#[test]
fn bp10_030_current_power_one_grants_aura_to_all_own_followers() {
    let mut initial = position("BP10-030", "hand");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"one","card":"BP01-037","state":{"power":1_u8}},
        {"id":"two","card":"BP01-037","state":{"power":2_u8}}
    ]);
    let mut game = loaded(&initial);
    play(&mut game, &[ModeId(1)]);
    let state = view(&game);
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert!(
        state["objects"]["one"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("オーラ"))
    );
    assert!(
        !state["objects"]["two"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("オーラ"))
    );
    let mut reduced = position("BP10-030", "field");
    reduced["players"]["P1"]["zones"]["field"][0]["state"] = json!({"power":1_u8});
    reduced["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"one","card":"BP01-037","state":{"power":1_u8}}));
    let reduced_game = loaded(&reduced);
    let reduced_view = view(&reduced_game);
    assert!(
        reduced_view["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("オーラ"))
    );
    assert!(
        reduced_view["objects"]["one"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("オーラ"))
    );
    assert!(
        !reduced_view["objects"]["enemy"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("オーラ"))
    );
}

#[test]
fn bp10_039_spellchain_twenty_resolves_every_threshold_and_both_buff_zones() {
    let mut initial = position("BP10-039", "hand");
    let mut cemetery: Vec<_> = (0_u8..20)
        .map(|i| json!({"id":format!("spell-{i}"),"card":"BP01-057"}))
        .collect();
    cemetery.push(json!({"id":"returned","card":"BP10-039"}));
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cemetery);
    initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
    assert_eq!(state["P1"]["leader"]["life"], 17_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["ex"], json!(["returned"]));
    for id in ["source", "returned"] {
        assert_eq!(state["objects"][id]["power"], 3_u8);
        assert_eq!(state["objects"][id]["hp"], 3_u8);
    }
}

#[test]
fn bp10_049_legal_evolution_pays_a_witch_discard_to_heal() {
    let mut initial = evolve_position("BP10-048", "BP10-049");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"cost","card":"BP01-057"}]);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    let state = view(&game);
    assert_eq!(state["P1"]["leader"]["life"], 15_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["cost"]));
}

#[test]
fn bp11_005_legal_evolution_uses_the_current_power_for_damage() {
    let mut initial = evolve_position("BP11-004", "BP11-005");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"power":7_u8});
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    let state = view(&game);
    assert_eq!(
        state["objects"]["enemy"]["damage"],
        state["objects"]["source"]["power"]
    );
    assert_eq!(state["objects"]["source"]["card"], "BP11-004");
    assert_eq!(state["objects"]["source"]["evolved"], true);
}

#[test]
fn bp11_081_attacking_a_follower_prevents_combat_damage() {
    let mut initial = evolve_position("BP11-080", "BP11-081");
    initial["players"]["P2"]["zones"]["field"][0]["state"]["power"] = json!(3_u8);
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
        .unwrap();
    finish(&mut game, None, &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 6_u8);
}

#[test]
fn bp11_002_a_vehicle_leaving_on_our_turn_creates_all_three_tokens_and_heals() {
    let mut initial = evolve_position("BP11-001", "BP11-002");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"horse","card":"BP11-T03"},{"id":"source","card":"BP11-001"}
    ]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"bounce","card":"BP01-015"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"draw","card":"BP01-171"}]);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    let step = game.play_internal_card("bounce", PLAYER).unwrap();
    finish(&mut game, Some(step), &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 3);
    let mut tokens: Vec<_> = state["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            state["objects"][id.as_str().unwrap()]["name"]
                .as_str()
                .unwrap()
        })
        .collect();
    tokens.sort_unstable();
    assert_eq!(tokens, ["従順な駿馬", "機動二輪車", "魔導装甲車"]);
}

#[test]
fn bp09_003_legal_evolution_keeps_the_power_thresholds_and_uses_power_for_attack_damage() {
    let mut initial = evolve_position("BP09-002", "BP09-003");
    initial["players"]["P1"]["zones"]["field"][0]["state"] = json!({"power":9_u8});
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ward","card":"BP01-159","state":{"acted":true}}));
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    let before_attack = view(&game);
    for keyword in ["守護", "疾走"] {
        assert!(
            before_attack["objects"]["source"]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!(keyword))
        );
    }
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
        .unwrap();
    finish(&mut game, None, &[]);
    let after_attack = view(&game);
    assert_eq!(after_attack["objects"]["enemy"]["damage"], 10_u8);
    assert_eq!(after_attack["P2"]["leader"]["life"], 10_u8);
}

#[test]
fn bp09_036_evolution_pays_the_stack_and_creates_the_named_elixir() {
    let mut initial = evolve_position("BP09-035", "BP09-036");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"soil","card":"BP01-075","state":{"counters":{"stack_counter":1_u8}}}));
    let mut game = loaded(&initial);
    evolve(&mut game, 1);
    let state = view(&game);
    let ex = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(ex.len(), 1);
    assert_eq!(
        state["objects"][ex[0].as_str().unwrap()]["card"],
        "BP09-T02"
    );
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("soil"))
    );
}

#[test]
fn bp10_081_cemetery_activation_banishes_itself_and_enters_the_advance_from_evolve_deck() {
    let mut initial = position("BP10-081", "cemetery");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"ghost","card":"BP03-074"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"advance","card":"BP10-077"}]);
    let mut game = loaded(&initial);
    activate_card(&mut game, "BP10-081", 1, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["banish"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 3);
    assert!(field.contains(&json!("advance")));
    assert_eq!(state["P1"]["ex"], json!(["ghost"]));
}

#[test]
fn bp10_049_non_witch_discard_mode_damages_the_selected_follower() {
    let mut initial = evolve_position("BP10-048", "BP10-049");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"neutral","card":"BP01-171"},{"id":"witch","card":"BP01-057"}]);
    let mut game = loaded(&initial);
    evolve_with_modes(&mut game, 1, &[ModeId(2)]);
    let state = view(&game);
    assert_eq!(state["objects"]["enemy"]["damage"], 5_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["neutral"]));
    assert_eq!(state["P1"]["hand"], json!(["witch"]));
}
