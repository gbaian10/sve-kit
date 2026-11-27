use super::ability_combinations::{activate_card, finish};
use super::*;

#[test]
fn chronos_schedules_an_extra_turn_and_is_banished_by_its_last_words() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"chronos","card":"GFD01-008EN"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-091"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("chronos", actor()).unwrap();
    assert!(matches!(play, CardPlay::Entered(_)));
    finish(&mut game, Some(play), &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "chronos"]));
    assert_eq!(
        state["semantic_state"]["turn_schedule"]["extra"],
        json!([{"player":"P1","count":1_u8}])
    );

    activate_card(&mut game, "BP01-091", 2, &[]);
    let after_destruction = game.projection(View::Referee).unwrap();
    assert_eq!(after_destruction["P1"]["banish"], json!(["chronos"]));
}

#[test]
fn filenes_fanfare_can_engage_multiple_followers_and_cannot_target_aura() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"filene","card":"GFD02-003EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"second","card":"BP01-037"},
        {"id":"shiva","card":"GFD02-008EN"}
    ]);
    let mut game = game(&initial);
    let play = game.play_internal_card("filene", actor()).unwrap();
    finish(&mut game, Some(play), &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["first"]["acted"], true);
    assert_eq!(state["objects"]["second"]["acted"], true);
    assert_eq!(state["objects"]["shiva"]["acted"], false);
}

#[test]
fn jafnhars_fanfare_evolves_it_and_resolves_the_printed_damage_ability() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"jafnhar","card":"GFD02-025EN"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFD02-026EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"target", "card":"BP01-037", "state":{"hp":5_u8,"max_hp":5_u8}
    }]);
    let mut game = game(&initial);
    let play = game.play_internal_card("jafnhar", actor()).unwrap();
    finish(&mut game, Some(play), &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["target"]["hp"], 1_u8);
    assert_eq!(state["objects"]["jafnhar"]["evolved_with"], "form");
}

#[test]
fn shiva_counts_the_amulet_flipped_to_pay_its_end_phase_ability() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"shiva","card":"GFD02-008EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"follower","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut evolve_deck = (0_u8..4)
        .map(|n| json!({"id":format!("open-{n}"),"card":"GFD02-026EN","face_up":true}))
        .collect::<Vec<_>>();
    evolve_deck.push(json!({"id":"amulet","card":"BP08-090"}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!(evolve_deck);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    finish(&mut game, None, &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["amulet"]["face_up"], true);
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
    assert_eq!(state["P2"]["field"], json!(["follower"]));
}

#[test]
fn lord_of_the_vajano_marsh_returns_enemy_followers_and_ex_cards_to_their_owners() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"lord","card":"GFD01-006EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"borrowed","card":"BP01-037","owner":"P1"},
        {"id":"amulet","card":"BP01-091"}
    ]);
    initial["players"]["P2"]["zones"]["ex"] =
        json!([{"id":"enemy-ex","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("lord", actor()).unwrap();
    finish(&mut game, Some(play), &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["borrowed"]), "the former controller does not take ownership");
    assert_eq!(state["P2"]["hand"], json!(["enemy-ex"]), "the enemy EX card also returns");
    assert_eq!(state["P2"]["field"], json!(["amulet"]), "the enemy field amulet is outside the follower filter");
    assert_eq!(state["P2"]["ex"], json!([]), "all selected EX cards already moved");
}

#[test]
fn hulking_dragonewt_changes_control_and_damages_its_new_controller() {
    use sve_engine::game::player::{Viewer, observation::ObservedZone};

    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"other-draw","card":"BP01-037"}]);
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"hulking","card":"GFD02-007EN"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"other-follower","card":"BP01-037","state":{"acted":true}}
    ]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"draw","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("hulking", actor()).unwrap();
    finish(&mut game, Some(play), &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["field"], json!(["hulking"]), "Fanfare transfers the follower");
    let observation = game.observe(Viewer::P1).unwrap();
    let transferred = observation.players[1]
        .zones
        .iter()
        .find(|zone| zone.kind == ObservedZone::Field)
        .unwrap();
    assert_eq!(
        transferred.cards[0].card.properties.owner,
        Viewer::P1,
        "the original owner is immutable"
    );
    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    finish(&mut game, None, &[]);
    let opponent_turn = game.projection(View::Referee).unwrap();
    assert_eq!(opponent_turn["turn"]["active"], "P2", "the follower's new controller gets their turn");
    assert!(opponent_turn["legal"].as_array().unwrap().iter().all(|action| {
        action["do"] != "attack" || action["attacker"] != "hulking"
    }), "the follower cannot attack its owner's leader or engaged follower");
    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    finish(&mut game, None, &[]);
    let after_trigger = game.projection(View::Referee).unwrap();
    assert_eq!(after_trigger["P2"]["leader"]["life"], 15_u8, "the current controller takes the end phase damage");
    assert_eq!(after_trigger["P1"]["leader"]["life"], 20_u8, "ownership does not redirect the end phase damage");
}

#[test]
fn mythcleaver_pays_an_english_counter_then_transfers_after_banishing() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"source","card":"GFD01-027EN"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"victim","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("source", actor()).unwrap();
    finish(&mut game, Some(play), &[]);
    let entered = game.projection(View::Referee).unwrap();
    assert_eq!(entered["objects"]["source"]["counters"]["myth counter"], 3_u8, "Fanfare places three English-named counters");
    activate_card(&mut game, "GFD01-027EN", 3, &[]);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["banish"], json!(["victim"]), "the selected enemy was banished");
    assert_eq!(after["P2"]["field"], json!(["source"]), "the selected enemy's controller received the amulet");
    assert_eq!(after["objects"]["source"]["counters"]["myth counter"], 2_u8, "the transferred amulet keeps the remaining counters");
    assert_eq!(after["objects"]["source"]["acted"], true, "the transferred amulet remains engaged");
}

#[test]
fn uriel_rewards_the_turn_player_with_an_angel_even_when_the_source_is_elsewhere() {
    for has_angel in [false, true] {
        let mut initial = draw_position();
        initial["turn"]["active"] = json!("P2");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"uriel","card":"GFD01-025EN"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"normal-draw","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["deck"] = json!([
            {"id":"reward-first","card":"BP01-037"},
            {"id":"reward-second","card":"BP01-037"}
        ]);
        if has_angel {
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"angel","card":"GFD01-025EN"}]);
        }
        let mut game = game(&initial);
        game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
        finish(&mut game, None, &[]);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 20_u8, "the source controller is not the end phase player");
        assert_eq!(after["P2"]["leader"]["life"], if has_angel {26_u8} else {20_u8}, "both Uriels reward P2 once when its field has an Angel");
        assert_eq!(after["P2"]["hand"].as_array().unwrap().len(), if has_angel {2} else {0}, "only the turn player with an Angel draws");
    }
}

#[test]
fn evolved_filene_counts_acted_followers_and_delays_their_refresh() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"filene","card":"GFD02-003EN"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"GFD02-004EN"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"additional","card":"BP01-037"},{"id":"reward","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"normal-draw","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"engaged","card":"BP01-037","state":{"acted":true}},
        {"id":"reserved","card":"BP01-037","state":{"acted":false}}
    ]);
    let mut game = game(&initial);
    let evolved = game.internal_rule_action(&json!({
        "do":"evolve","source":"filene","evolve_card":"form","pay":{"pp":1_u8}
    })).unwrap();
    assert_eq!(evolved.outcome, "resolved", "the real evolve ability attaches Filene's form");
    finish(&mut game, None, &[]);
    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    finish(&mut game, None, &[]);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"], json!(["additional"]), "the end phase counts exactly one engaged enemy follower");
    assert_eq!(after["turn"]["active"], "P2", "the opposing start procedure completed");
    assert_eq!(after["objects"]["engaged"]["acted"], true, "the existing engaged enemy cannot refresh");
    assert_eq!(after["objects"]["reserved"]["acted"], false, "the existing reserved enemy stays reserved");
}

#[test]
fn yggdrasils_printed_last_words_prevent_damage_from_the_next_attack() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"BP01-091"},
        {"id":"yggdrasil","card":"GFD01-002EN"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"GFD01-003EN"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"raider","card":"BP01-037","state":{"power":3_u8}}]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"normal-draw","card":"BP01-037"}]);
    let mut game = game(&initial);
    let evolved = game.internal_rule_action(&json!({
        "do":"evolve","source":"yggdrasil","evolve_card":"form","pay":{"pp":1_u8}
    })).unwrap();
    assert_eq!(evolved.outcome, "resolved", "the printed evolve ability attaches Yggdrasil's form");
    finish(&mut game, None, &[]);
    activate_card(&mut game, "BP01-091", 2, &[]);
    let destroyed = game.projection(View::Referee).unwrap();
    assert!(destroyed["P1"]["cemetery"].as_array().unwrap().contains(&json!("yggdrasil")), "the evolved follower reached the cemetery");
    game.internal_rule_action(&json!({"do":"end-phase"})).unwrap();
    finish(&mut game, None, &[]);
    assert_eq!(game.projection(View::Referee).unwrap()["turn"]["active"], "P2", "the enemy reaches the next main phase");
    let attack = game.internal_rule_action(&json!({"do":"attack","attacker":"raider","target":"P1.leader"})).unwrap();
    assert_eq!(attack.outcome, "resolved", "the leader is an eligible target after the Ward follower leaves");
    finish(&mut game, None, &[]);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["raider"]["acted"], true, "the attack reached combat");
    assert_eq!(after["P1"]["leader"]["life"], 20_u8, "the departed Yggdrasil prevents its controller from losing defense");
}
