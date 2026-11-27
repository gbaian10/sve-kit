use super::ability_combinations::finish;
use super::admission_bp_cp_fixture::{actor, evolve, game, keyword, play, position, settle};
use super::english_cards::{attack, settle_all, state, take_all};
use super::*;

fn goblins(prefix: &str, count: usize) -> Value {
    json!(
        (0..count)
            .map(|index| json!({"id":format!("{prefix}-{index}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    )
}

fn end_phase(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(board);
}

fn evolved_on_field(initial: &mut Value, base: &str, form: &str) {
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source","card":base,"state":{"evolved":true,"evolved_with":"form"}
    }]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":form}]);
}

#[test]
fn milnard_makes_an_opponent_with_seven_cards_discard_three() {
    for (held, kept) in [(7_usize, 4_usize), (6, 6)] {
        let mut initial = position("GFB01c-001EN");
        initial["players"]["P2"]["zones"]["hand"] = goblins("held", held);
        let mut board = game(&initial);
        play(&mut board, "source");
        let after = state(&board);
        assert_eq!(
            after["P2"]["hand"].as_array().unwrap().len(),
            kept,
            "{held} held"
        );
    }
}

#[test]
fn evolved_milnard_draws_two_and_recovers_three_on_strike() {
    let mut initial = position("GFB01c-001EN");
    evolved_on_field(&mut initial, "GFB01c-001EN", "GFB01c-002EN");
    initial["players"]["P1"]["pp"] = json!({"current":2_u8,"max":10_u8});
    initial["players"]["P1"]["zones"]["deck"] = goblins("deck", 3);
    let mut board = game(&initial);
    attack(&mut board, "source", "P2.leader");
    let after = state(&board);
    assert_eq!(after["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(after["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn orb_of_desecration_buries_the_top_card_at_end() {
    let mut initial = position("GFB01c-007EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01c-007EN"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"top","card":"BP01-171"},{"id":"next","card":"BP01-171"}]);
    let mut board = game(&initial);
    end_phase(&mut board);
    assert_eq!(state(&board)["P1"]["cemetery"], json!(["top"]));
}

#[test]
fn orb_of_desecration_revives_a_follower_from_a_cemetery_of_twenty() {
    let mut initial = position("GFB01c-007EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01c-007EN"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = goblins("grave", 20);
    let mut revived = game(&initial);
    let step = revived
        .activate_internal_card("source", activated("GFB01c-007EN"), actor())
        .unwrap();
    take_all(&mut revived, step);
    settle(&mut revived);
    let after = state(&revived);
    assert!(
        after["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("grave-0")),
        "{after}"
    );
    assert_eq!(after["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn orb_of_desecration_cannot_revive_with_nineteen_cards_in_the_cemetery() {
    let mut initial = position("GFB01c-007EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01c-007EN"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = goblins("grave", 19);
    let mut board = game(&initial);
    board
        .activate_internal_card("source", activated("GFB01c-007EN"), actor())
        .unwrap_err();
}

#[test]
fn emeralda_destroys_a_cheap_enemy_and_buries_two() {
    let mut initial = position("GFB01c-018EN");
    initial["players"]["P1"]["zones"]["deck"] = goblins("deck", 3);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let after = state(&board);
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(after["P1"]["cemetery"].as_array().unwrap().len(), 2);
    assert!(keyword(&after, "source", "疾走"));
}

#[test]
fn evolved_emeralda_buries_two_on_strike() {
    let mut initial = position("GFB01c-018EN");
    evolved_on_field(&mut initial, "GFB01c-018EN", "GFB01c-019EN");
    initial["players"]["P1"]["zones"]["deck"] = goblins("deck", 3);
    let mut board = game(&initial);
    attack(&mut board, "source", "P2.leader");
    let after = state(&board);
    assert_eq!(after["P1"]["cemetery"], json!(["deck-0", "deck-1"]));
}

#[test]
fn cursebrand_vampire_scales_with_necrocharge() {
    for (grave, enemy_damage, leader) in [(0_usize, 2_u8, 20_u8), (10, 5, 20), (20, 5, 17)] {
        let mut initial = position("GFB01c-020EN");
        initial["players"]["P1"]["zones"]["cemetery"] = goblins("grave", grave);
        initial["players"]["P2"]["zones"]["field"] = json!([{
            "id":"enemy","card":"BP01-037","state":{"hp":30_u8,"max_hp":30_u8}
        }]);
        let mut board = game(&initial);
        play(&mut board, "source");
        let after = state(&board);
        assert_eq!(
            after["objects"]["enemy"]["damage"], enemy_damage,
            "{grave} in cemetery"
        );
        assert_eq!(after["P2"]["leader"]["life"], leader, "{grave} in cemetery");
    }
}

#[test]
fn evolved_garula_deals_damage_equal_to_amulets_at_end() {
    let mut initial = position("GFB01d-003EN");
    evolved_on_field(&mut initial, "GFB01d-003EN", "GFB01d-004EN");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"fangs-a","card":"GFB01d-024EN"}),
            json!({"id":"fangs-b","card":"GFB01d-024EN"}),
        ]);
    let mut board = game(&initial);
    end_phase(&mut board);
    assert_eq!(state(&board)["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn elysian_saint_hares_can_return_a_banished_card() {
    let mut initial = position("GFB01d-006EN");
    initial["players"]["P1"]["zones"]["banish"] = json!([{"id":"gone","card":"BP01-171"}]);
    let mut board = game(&initial);
    let step = board.play_internal_card("source", actor()).unwrap();
    finish(&mut board, Some(step), &[ModeId(2)]);
    assert_eq!(state(&board)["P1"]["hand"], json!(["gone"]));
}

#[test]
fn evolved_elysian_saint_hares_rewards_the_higher_roller() {
    for (dice, mine, theirs) in [([5_u8, 2_u8], 23_u8, 20_u8), ([3, 3], 23, 23)] {
        let mut initial = position("GFB01d-006EN");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01d-006EN"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"GFB01d-007EN"}]);
        initial["players"]["P1"]["zones"]["deck"] = goblins("mine", 3);
        initial["players"]["P2"]["zones"]["deck"] = goblins("theirs", 3);
        let mut board = Game::from_selected_effects(
            selection().clone(),
            &initial,
            &Value::Null,
            &json!({"dice":dice}),
            "elysian",
        )
        .unwrap();
        evolve(&mut board, "form", 1);
        let after = state(&board);
        assert_eq!(after["P1"]["leader"]["life"], mine, "{dice:?}");
        assert_eq!(after["P2"]["leader"]["life"], theirs, "{dice:?}");
        // Saint-Hares draws first; each highest roller draws again.
        let hand = |seat: &str| after[seat]["hand"].as_array().unwrap().len();
        assert_eq!(hand("P1"), 2, "{dice:?}");
        assert_eq!(hand("P2"), usize::from(theirs == 23), "{dice:?}");
    }
}

#[test]
fn valhallan_general_resets_its_defense_to_four_at_end() {
    let mut initial = position("GFB01d-019EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source","card":"GFB01d-019EN","state":{"hp":8_u8,"max_hp":8_u8}
    }]);
    let mut board = game(&initial);
    end_phase(&mut board);
    assert_eq!(state(&board)["objects"]["source"]["hp"], 4_u8);
}

#[test]
fn evolved_valhallan_general_engages_enemies_and_resets_to_five() {
    let mut initial = position("GFB01d-019EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01d-019EN"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFB01d-020EN"}]);
    initial["players"]["P2"]["zones"]["field"] = goblins("enemy", 2);
    let mut evolved = game(&initial);
    evolve(&mut evolved, "form", 1);
    let after = state(&evolved);
    for id in ["enemy-0", "enemy-1"] {
        assert_eq!(after["objects"][id]["acted"], true, "{id}");
    }
    end_phase(&mut evolved);
    assert_eq!(state(&evolved)["objects"]["source"]["hp"], 5_u8);
}

#[test]
fn greater_priestess_banishes_a_card_in_the_opponents_ex_area() {
    let mut initial = position("GFB01d-021EN");
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"stored","card":"BP01-171"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let after = state(&board);
    assert_eq!(after["P2"]["ex"], json!([]));
    assert_eq!(after["P2"]["banish"], json!(["stored"]));
}

#[test]
fn grimaud_recovers_two_play_points_per_amulet() {
    let mut initial = position("GFB01d-022EN");
    initial["players"]["P1"]["zones"]["field"] = json!([
        {"id":"fangs-a","card":"GFB01d-024EN"},
        {"id":"fangs-b","card":"GFB01d-024EN"}
    ]);
    let mut board = game(&initial);
    play(&mut board, "source");
    assert_eq!(state(&board)["P1"]["pp"]["current"], 8_u8);
}

#[test]
fn noble_fangs_uses_an_allys_attack_and_buries_itself_to_hit_the_leader_too() {
    let mut initial = position("GFB01d-024EN");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"enemy","card":"BP01-037","state":{"hp":30_u8,"max_hp":30_u8}
    }]);
    let mut board = game(&initial);
    play(&mut board, "source");
    assert_eq!(state(&board)["objects"]["enemy"]["damage"], 5_u8);

    let step = board
        .activate_internal_card("source", activated("GFB01d-024EN"), actor())
        .unwrap();
    take_all(&mut board, step);
    settle_all(&mut board);
    let after = state(&board);
    assert_eq!(after["objects"]["enemy"]["damage"], 10_u8, "{after}");
    assert_eq!(after["P2"]["leader"]["life"], 15_u8);
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}

#[test]
fn new_years_soul_devil_enters_without_a_game_action() {
    let initial = position("NY2024-001EN");
    let mut board = game(&initial);
    let before = state(&board);
    play(&mut board, "source");
    let after = state(&board);
    assert_eq!(after["P1"]["field"], json!(["source"]));
    assert_eq!(after["P1"]["hand"], json!([]));
    assert_eq!(after["P1"]["deck"], before["P1"]["deck"]);
    assert_eq!(after["P2"], before["P2"]);
}

fn spawn_position(card: &str, class: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["leader"]["class"] = json!(class);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = goblins("deck", 3);
    initial["players"]["P2"]["zones"]["hand"] = goblins("held", 3);
    initial["players"]["P2"]["zones"]["field"] = goblins("enemy", 3);
    initial
}

#[test]
fn spawn_of_the_abyss_works_only_with_an_abysscraft_leader() {
    for (class, life, hand, their_hand, their_field) in [
        ("ナイトメア", 22_u8, 1_usize, 2_usize, 2_usize),
        ("ニュートラル", 20, 0, 3, 3),
    ] {
        let mut board = game(&spawn_position("GFB01c-005EN", class));
        end_phase(&mut board);
        let after = state(&board);
        assert_eq!(after["P1"]["leader"]["life"], life, "{class}");
        assert_eq!(
            after["P1"]["hand"].as_array().unwrap().len(),
            hand,
            "{class}"
        );
        assert_eq!(
            after["P2"]["hand"].as_array().unwrap().len(),
            their_hand,
            "{class}"
        );
        assert_eq!(
            after["P2"]["field"].as_array().unwrap().len(),
            their_field,
            "{class}"
        );
    }
}

#[test]
fn evolved_spawn_of_the_abyss_doubles_the_abysscraft_end_phase() {
    let mut initial = spawn_position("GFB01c-005EN", "ナイトメア");
    evolved_on_field(&mut initial, "GFB01c-005EN", "GFB01c-006EN");
    let mut board = game(&initial);
    end_phase(&mut board);
    let after = state(&board);
    assert_eq!(after["P1"]["leader"]["life"], 24_u8);
    assert_eq!(after["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(after["P2"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(after["P2"]["field"].as_array().unwrap().len(), 1);
}

#[test]
fn garula_summons_a_three_and_a_one_cost_amulet_for_a_havencraft_leader() {
    for (class, summoned) in [("ビショップ", true), ("ロイヤル", false)] {
        let mut initial = position("GFB01d-003EN");
        initial["players"]["P1"]["leader"]["class"] = json!(class);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"fangs","card":"GFB01d-024EN"},
            {"id":"orb","card":"GFB01c-007EN"},
            {"id":"goblin","card":"BP01-171"}
        ]);
        let mut board = game(&initial);
        let step = board.play_internal_card("source", actor()).unwrap();
        take_all(&mut board, step);
        settle_all(&mut board);
        let field = state(&board)["P1"]["field"].clone();
        let expected = if summoned {
            json!(["source", "fangs", "orb"])
        } else {
            json!(["source"])
        };
        assert_eq!(field, expected, "{class}");
    }
}

#[test]
fn milnard_evolves_only_with_two_or_fewer_cards_in_hand() {
    for (held, allowed) in [(2_usize, true), (3, false)] {
        let mut initial = position("GFB01c-001EN");
        initial["players"]["P1"]["zones"]["hand"] = goblins("held", held);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"GFB01c-001EN"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"form","card":"GFB01c-002EN"}]);
        let mut board = game(&initial);
        let outcome = board.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        );
        let resolved = outcome.is_ok_and(|result| result.outcome == "resolved");
        assert_eq!(resolved, allowed, "{held} in hand");
    }
}

#[test]
fn elysian_saint_hares_can_summon_a_one_cost_follower_from_the_deck() {
    let mut initial = position("GFB01d-006EN");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"big","card":"BP01-037"},{"id":"goblin","card":"BP01-171"}]);
    let mut board = game(&initial);
    let step = board.play_internal_card("source", actor()).unwrap();
    finish(&mut board, Some(step), &[ModeId(1)]);
    let after = state(&board);
    assert_eq!(after["P1"]["field"], json!(["source", "goblin"]));
    assert_eq!(after["P1"]["deck"], json!(["big"]));
}

#[test]
fn demonic_storm_uses_a_cemetery_demons_attack_and_summons_it_at_twenty() {
    for (fillers, summoned) in [(19_usize, true), (4, false)] {
        let mut initial = position("GFB01c-021EN");
        let mut grave = vec![json!({"id":"demon","card":"BP01-101"})];
        grave.extend(
            goblins("grave", fillers)
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
        initial["players"]["P1"]["zones"]["cemetery"] = json!(grave);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"tough","card":"BP01-037","state":{"hp":30_u8,"max_hp":30_u8}},
            {"id":"goblin","card":"BP01-171"}
        ]);
        let mut board = game(&initial);
        let step = board.play_internal_card("source", actor()).unwrap();
        take_all(&mut board, step);
        settle_all(&mut board);
        let after = state(&board);
        assert_eq!(after["objects"]["tough"]["damage"], 3_u8, "{after}");
        assert_eq!(after["P2"]["cemetery"], json!(["goblin"]));
        assert_eq!(
            after["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("demon")),
            summoned,
            "{} in cemetery",
            fillers + 1
        );
    }
}
