use super::admission_bp_cp_fixture::{
    actor, destruction_game, evolve, game, keyword, next_main, play, position, settle,
};
use super::attack_batch::end_turn;
use super::english_cards::{attack, settle_all, state, take_all};
use super::*;

#[test]
fn magnolia_searches_a_cheap_follower_and_its_evolution_buffs_the_others() {
    let mut initial = position("GFB01a-001EN");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"big","card":"BP01-037"},{"id":"cheap","card":"BP01-173"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-042"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFB01a-002EN"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let searched = state(&board);
    assert_eq!(searched["P1"]["hand"], json!(["cheap"]));
    assert_eq!(searched["P1"]["deck"], json!(["big"]));

    evolve(&mut board, "form", 3);
    let evolved = state(&board);
    assert_eq!(
        (
            &evolved["objects"]["ally"]["power"],
            &evolved["objects"]["ally"]["hp"]
        ),
        (&json!(4_u8), &json!(4_u8)),
        "{evolved}"
    );
}

#[test]
fn latham_recovers_only_swordcraft_followers_and_keeps_an_oversized_hand() {
    let mut initial = position("GFB01a-003EN");
    let mut hand = (0_u8..8)
        .map(|index| json!({"id":format!("held-{index}"),"card":"BP01-171"}))
        .collect::<Vec<_>>();
    hand.push(json!({"id":"source","card":"GFB01a-003EN"}));
    initial["players"]["P1"]["zones"]["hand"] = json!(hand);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"swordcraft","card":"BP01-042"},
        {"id":"neutral","card":"BP01-173"}
    ]);
    let mut board = game(&initial);
    play(&mut board, "source");
    next_main(&mut board);
    let after = state(&board);
    assert_eq!(after["turn"]["active"], "P2");
    assert_eq!(after["P1"]["hand"].as_array().unwrap().len(), 9);
    assert!(
        after["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("swordcraft"))
    );
    assert_eq!(after["P1"]["cemetery"], json!(["neutral"]));
}

#[test]
fn lathams_evolution_turns_face_up_evolved_followers_face_down() {
    let mut initial = position("GFB01a-003EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01a-003EN"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([
        {"id":"form","card":"GFB01a-004EN"},
        {"id":"used","card":"BP01-027","face_up":true},
        {"id":"hidden","card":"BP01-030","face_up":false}
    ]);
    let mut board = game(&initial);
    board
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["objects"]["used"]["face_up"], false, "{after}");
    assert_eq!(after["objects"]["hidden"]["face_up"], false);
}

#[test]
fn sky_fortress_last_words_summon_two_cheap_followers_from_the_cemetery() {
    let mut initial = position("GFB01a-006EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01a-006EN"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-173"},
        {"id":"third","card":"BP01-042"},
        {"id":"too-big","card":"BP01-037"}
    ]);
    let mut board = destruction_game(&initial);
    activate(&mut board, "P1");
    settle(&mut board);
    let after = state(&board);
    let field = after["P1"]["field"].as_array().unwrap();
    assert_eq!(
        field.iter().filter(|id| *id != "producer").count(),
        2,
        "Sky Fortress summons up to two followers"
    );
    assert!(!field.contains(&json!("too-big")));
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}

#[test]
fn evolved_sky_fortress_last_words_summon_three_cheap_followers() {
    let mut initial = position("GFB01a-006EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source","card":"GFB01a-006EN","state":{"evolved":true,"evolved_with":"form"}
    }]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"GFB01a-007EN"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-173"},
        {"id":"third","card":"BP01-042"},
        {"id":"too-big","card":"BP01-037"}
    ]);
    let mut board = destruction_game(&initial);
    activate(&mut board, "P1");
    settle(&mut board);
    let after = state(&board);
    let field = after["P1"]["field"].as_array().unwrap();
    for id in ["first", "second", "third"] {
        assert!(field.contains(&json!(id)), "{id} returns to the field");
    }
    assert!(!field.contains(&json!("too-big")));
}

#[test]
fn odin_banishes_one_card_on_the_opponents_field() {
    let mut initial = position("GFB01a-008EN");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let after = state(&board);
    assert_eq!(after["P2"]["field"], json!([]));
    assert_eq!(after["P2"]["banish"], json!(["enemy"]));
    assert!(keyword(&after, "source", "疾走"));
}

#[test]
fn blitz_lancer_rushes_an_engaged_enemy_follower() {
    let mut initial = position("GFB01a-017EN");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFB01a-018EN"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-171","state":{"acted":true}}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let played = state(&board);
    assert!(keyword(&played, "source", "突進"));
    let outcome = attack(&mut board, "source", "enemy");
    let fought = state(&board);
    assert_eq!(
        fought["P2"]["cemetery"],
        json!(["enemy"]),
        "{outcome}: {fought}"
    );
}

#[test]
fn evolved_blitz_lancer_has_storm() {
    let mut initial = position("GFB01a-017EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01a-017EN"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFB01a-018EN"}]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 1);
    assert!(keyword(&state(&board), "source", "疾走"));
}

#[test]
fn giantsbane_destroys_an_expensive_enemy_and_grows() {
    let mut initial = position("GFB01a-019EN");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"giant","card":"BP01-001"},{"id":"small","card":"BP01-171"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let after = state(&board);
    assert_eq!(after["P2"]["field"], json!(["small"]));
    assert_eq!(after["P2"]["cemetery"], json!(["giant"]));
    assert_eq!(after["objects"]["source"]["power"], 6_u8);
}

#[test]
fn noble_knight_draws_two_and_summons_cheap_followers_from_hand() {
    let mut initial = position("GFB01a-020EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"GFB01a-020EN"},
        {"id":"cheap","card":"BP01-042"},
        {"id":"big","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"one","card":"BP01-171"},{"id":"two","card":"BP01-037"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let after = state(&board);
    let field = after["P1"]["field"].as_array().unwrap();
    assert!(field.contains(&json!("cheap")));
    assert!(
        field.contains(&json!("one")),
        "a drawn cheap follower may also enter"
    );
    assert!(
        after["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("big"))
    );
    assert!(
        after["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("two"))
    );
}

#[test]
fn ironwrought_fortress_may_summon_a_shield_guardian_at_end_phase() {
    let mut initial = position("GFB01a-021EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01a-021EN"}]);
    let mut board = game(&initial);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(&mut board);
    let ended = state(&board);
    assert_eq!(ended["P1"]["field"].as_array().unwrap().len(), 2);
}

#[test]
fn ironwrought_fortress_buries_itself_to_give_defense() {
    let mut initial = position("GFB01a-021EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01a-021EN"}]);
    let mut board = game(&initial);
    let step = board
        .activate_internal_card("source", activated("GFB01a-021EN"), actor())
        .unwrap();
    take_all(&mut board, step);
    settle(&mut board);
    let after = state(&board);
    assert_eq!(after["P1"]["leader"]["life"], 23_u8);
    assert_eq!(after["P1"]["cemetery"], json!(["source"]));
}

#[test]
fn altaia_damages_only_neutral_enemy_followers() {
    let mut initial = position("GFB01a-022EN");
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"neutral","card":"BP01-173"},
        {"id":"swordcraft","card":"BP01-042"}
    ]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let after = state(&board);
    assert_eq!(after["P2"]["field"], json!(["swordcraft"]));
    assert_eq!(after["P2"]["cemetery"], json!(["neutral"]));
}

#[test]
fn tisiphone_refreshes_after_striking_a_follower_but_attacks_only_once() {
    let mut initial = position("GFB01b-001EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01b-001EN"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}]);
    let mut board = game(&initial);
    assert_eq!(attack(&mut board, "source", "first"), "resolved");
    let after = state(&board);
    assert_eq!(after["objects"]["source"]["acted"], false);
    assert_eq!(after["objects"]["source"]["damage"], 0_u8);
    assert_eq!(
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"second"}))
            .unwrap()
            .outcome,
        "cannot-attack",
        "one remaining opponent allows one attack per turn"
    );
}

#[test]
fn evolved_tisiphone_refreshes_after_striking_the_leader() {
    let mut initial = position("GFB01b-001EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source","card":"GFB01b-001EN","state":{"evolved":true,"evolved_with":"form"}
    }]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"GFB01b-002EN"}]);
    let mut board = game(&initial);
    attack(&mut board, "source", "P2.leader");
    let after = state(&board);
    assert_eq!(after["objects"]["source"]["acted"], false);
    assert!(after["P2"]["leader"]["life"].as_u64().unwrap() < 20);
}

#[test]
fn alecto_stores_followers_in_ex_and_buffs_field_and_ex_at_end() {
    let mut initial = position("GFB01b-003EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"GFB01b-003EN"},
        {"id":"stored","card":"BP01-171"},
        {"id":"spell","card":"BP01-001"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-042"}]);
    let mut board = game(&initial);
    let step = board.play_internal_card("source", actor()).unwrap();
    take_all(&mut board, step);
    settle_all(&mut board);
    let played = state(&board);
    assert!(
        played["P1"]["ex"]
            .as_array()
            .unwrap()
            .contains(&json!("stored")),
        "{played}"
    );
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(&mut board);
    let ended = state(&board);
    assert_eq!(ended["objects"]["ally"]["power"], 4_u8);
    assert_eq!(ended["objects"]["stored"]["power"], 4_u8);
}

#[test]
fn evolved_alecto_discounts_a_follower_in_ex_this_turn() {
    let mut initial = position("GFB01b-003EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01b-003EN"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFB01b-004EN"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"stored","card":"BP01-037"}]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 1);
    play(&mut board, "stored");
    assert_eq!(state(&board)["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn megaera_deals_ten_to_a_follower_at_end_and_the_leader_at_main() {
    let mut initial = position("GFB01b-005EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01b-005EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    next_main(&mut board);
    assert_eq!(state(&board)["P2"]["cemetery"], json!(["enemy"]));
    next_main(&mut board);
    assert_eq!(state(&board)["P2"]["leader"]["life"], 10_u8);
}

#[test]
fn malocchios_strike_destroys_and_damages_at_its_counter_thresholds() {
    let mut initial = position("GFB01b-006EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source","card":"GFB01b-006EN",
        "state":{"evolved":true,"evolved_with":"form","counters":{"armageddon_counter":20_u8}}
    }]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"GFB01b-007EN"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037"},{"id":"other","card":"BP01-171"}]);
    let mut board = game(&initial);
    attack(&mut board, "source", "P2.leader");
    let after = state(&board);
    assert_eq!(after["P2"]["field"], json!([]));
    assert!(after["P2"]["leader"]["life"].as_u64().unwrap() <= 10);
}

#[test]
fn lokis_next_turn_doubles_a_fanfare() {
    let mut initial = position("GFB01b-008EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"GFB01b-008EN"},
        {"id":"commander","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"d1","card":"BP01-171"},{"id":"d2","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["deck"] =
        json!([{"id":"e1","card":"BP01-171"},{"id":"e2","card":"BP01-171"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    end_turn(&mut board);
    end_turn(&mut board);
    let turn = state(&board);
    assert_eq!(turn["turn"]["active"], "P1", "{turn}");
    play(&mut board, "commander");
    assert_eq!(state(&board)["objects"]["ally"]["power"], 4_u8);
}

#[test]
fn helblindi_gives_low_rolls_to_the_roller_and_high_rolls_to_its_controller() {
    let mut initial = position("GFB01b-024EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01b-024EN"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..4)
            .map(|index| json!({"id":format!("mine-{index}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["deck"] = json!(
        (0_u8..4)
            .map(|index| json!({"id":format!("theirs-{index}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut board = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &json!({"dice":[2_u8, 5_u8]}),
        "helblindi",
    )
    .unwrap();
    next_main(&mut board);
    let low = state(&board);
    assert_eq!(
        low["P2"]["leader"]["life"], 21_u8,
        "a low roll heals the roller"
    );
    next_main(&mut board);
    let high = state(&board);
    assert_eq!(
        high["P1"]["leader"]["life"], 21_u8,
        "a high roll heals Helblindi's controller"
    );
}

#[test]
fn sky_fortress_pays_full_cost_with_nine_cards_on_the_field() {
    // Two-player fields hold five cards each (CR 4.4.4.1), so ten cards leave no room to play it.
    let mut initial = position("GFB01a-006EN");
    initial["players"]["P1"]["zones"]["field"] = json!(
        (0_u8..4)
            .map(|index| json!({"id":format!("mine-{index}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!(
        (0_u8..5)
            .map(|index| json!({"id":format!("theirs-{index}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    let mut board = game(&initial);
    play(&mut board, "source");
    let after = state(&board);
    assert_eq!(after["P1"]["pp"]["current"], 3_u8, "{after}");
}

#[test]
fn tisiphone_evolves_at_end_only_after_three_enemy_followers_died() {
    for (enemies, evolves) in [(2_u8, false), (3, true)] {
        let mut initial = position("GFB01a-022EN");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"tisiphone","card":"GFB01b-001EN"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"GFB01b-002EN"}]);
        initial["players"]["P2"]["zones"]["field"] = json!(
            (0..enemies)
                .map(|index| json!({"id":format!("goblin-{index}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut board = game(&initial);
        play(&mut board, "source");
        assert_eq!(state(&board)["P2"]["field"], json!([]));
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        settle(&mut board);
        let ended = state(&board);
        assert_eq!(
            ended["objects"]["tisiphone"]["evolved_with"] == "form",
            evolves,
            "{enemies} destroyed: {ended}"
        );
    }
}

#[test]
fn malocchio_with_thirty_counters_makes_the_opponent_lose() {
    let mut initial = position("GFB01b-006EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source","card":"GFB01b-006EN",
        "state":{"evolved":true,"evolved_with":"form","counters":{"armageddon_counter":30_u8}}
    }]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"GFB01b-007EN"}]);
    let mut board = game(&initial);
    attack(&mut board, "source", "P2.leader");
    let after = state(&board);
    assert_eq!(after["game"]["ended"], true, "{after}");
}

#[test]
fn acceleratium_finds_a_machina_card_and_buries_itself_to_rush_and_refund() {
    let mut initial = position("GFB01b-022EN");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"goblin","card":"BP01-171"},{"id":"mech","card":"BP07-110"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    assert_eq!(state(&board)["P1"]["hand"], json!(["mech"]));
}

#[test]
fn acceleratium_refunds_play_points_equal_to_the_machinas_cost() {
    let mut initial = position("GFB01b-022EN");
    initial["players"]["P1"]["pp"] = json!({"current":0_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"source","card":"GFB01b-022EN"},
        {"id":"drone","card":"BP07-110","state":{"entered_this_turn":true}}
    ]);
    let mut board = game(&initial);
    let step = board
        .activate_internal_card("source", activated("GFB01b-022EN"), actor())
        .unwrap();
    take_all(&mut board, step);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["P1"]["cemetery"], json!(["source"]));
    assert!(keyword(&after, "drone", "突進"));
    assert_eq!(after["P1"]["pp"]["current"], 2_u8);
}
