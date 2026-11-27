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

fn on_field(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn evolved_on_field(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{
        "id":"source","card":base,"state":{"evolved":true,"evolved_with":"form"}
    }]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":form}]);
    initial
}

fn play_all(board: &mut Game, id: &str) {
    let step = board.play_internal_card(id, actor()).unwrap();
    take_all(board, step);
    settle_all(board);
}

#[test]
fn arthur_summons_cheap_followers_from_the_top_ten_for_a_swordcraft_leader() {
    for (class, summoned) in [("ロイヤル", true), ("ニュートラル", false)] {
        let mut initial = position("GFB01a-005EN");
        initial["players"]["P1"]["leader"]["class"] = json!(class);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"cheap","card":"BP01-042"},
            {"id":"big","card":"BP01-037"},
            {"id":"goblin","card":"BP01-171"}
        ]);
        let mut board = game(&initial);
        play_all(&mut board, "source");
        let after = state(&board);
        let field = after["P1"]["field"].as_array().unwrap();
        assert_eq!(
            field.contains(&json!("cheap")),
            summoned,
            "{class}: {after}"
        );
        assert_eq!(field.contains(&json!("goblin")), summoned, "{class}");
        assert!(!field.contains(&json!("big")), "{class}");
        assert_eq!(
            after["P1"]["deck"].as_array().unwrap().len(),
            if summoned { 1 } else { 3 }
        );
    }
}

#[test]
fn burst_shot_mode_one_draws_then_deals_damage_equal_to_the_hand() {
    let mut initial = position("GFB01a-016EN");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"GFB01a-016EN"},
        {"id":"held","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"enemy","card":"BP01-037","state":{"hp":30_u8,"max_hp":30_u8}
    }]);
    let mut board = game(&initial);
    let step = board.play_internal_card("source", actor()).unwrap();
    finish(&mut board, Some(step), &[ModeId(1)]);
    assert_eq!(state(&board)["objects"]["enemy"]["damage"], 2_u8);
}

#[test]
fn burst_shot_mode_two_needs_a_forestcraft_leader() {
    for (class, damage, life) in [("エルフ", 2_u8, 18_u8), ("ロイヤル", 0, 20)] {
        let mut initial = position("GFB01a-016EN");
        initial["players"]["P1"]["leader"]["class"] = json!(class);
        initial["players"]["P1"]["zones"]["hand"] = json!([
            {"id":"source","card":"GFB01a-016EN"},
            {"id":"held","card":"BP01-171"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([{
            "id":"enemy","card":"BP01-037","state":{"hp":30_u8,"max_hp":30_u8}
        }]);
        let mut board = game(&initial);
        let step = board.play_internal_card("source", actor()).unwrap();
        finish(&mut board, Some(step), &[ModeId(2)]);
        let after = state(&board);
        assert_eq!(after["objects"]["enemy"]["damage"], damage, "{class}");
        assert_eq!(after["P2"]["leader"]["life"], life, "{class}");
    }
}

#[test]
fn grand_summoning_adds_a_third_follower_and_play_points_for_a_runecraft_leader() {
    for (class, field, pp) in [("ウィッチ", 3_usize, 4_u8), ("ロイヤル", 2, 2)] {
        let mut initial = position("GFB01b-021EN");
        initial["players"]["P1"]["leader"]["class"] = json!(class);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"three","card":"BP01-042"},
            {"id":"five","card":"BP01-037"},
            {"id":"seven","card":"BP01-037"},
            {"id":"queen","card":"BP01-001"}
        ]);
        let mut board = game(&initial);
        play_all(&mut board, "source");
        let after = state(&board);
        assert_eq!(
            after["P1"]["field"].as_array().unwrap().len(),
            field,
            "{class}: {after}"
        );
        assert_eq!(after["P1"]["pp"]["current"], pp, "{class}");
    }
}

#[test]
fn dragonrend_quake_is_cheaper_for_dragoncraft_and_trades_attacks() {
    for (class, pp) in [("ドラゴン", 9_u8), ("ロイヤル", 6)] {
        let mut initial = position("GFB01b-023EN");
        initial["players"]["P1"]["leader"]["class"] = json!(class);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"big","card":"BP01-037","state":{"hp":30_u8,"max_hp":30_u8}},
            {"id":"small","card":"BP01-042","state":{"hp":30_u8,"max_hp":30_u8}}
        ]);
        let mut board = game(&initial);
        let step = board.play_internal_card("source", actor()).unwrap();
        let step = choose_card_by_number(&mut board, step, "BP01-037");
        let step = choose_card_by_number(&mut board, step, "BP01-042");
        take_all(&mut board, step);
        settle_all(&mut board);
        let after = state(&board);
        assert_eq!(after["P1"]["pp"]["current"], pp, "{class}");
        assert_eq!(after["objects"]["big"]["damage"], 2_u8, "{class}");
        assert_eq!(after["objects"]["small"]["damage"], 5_u8, "{class}");
    }
}

#[test]
fn aramis_grows_with_other_officers() {
    let mut initial = position("GFE01-003EN");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"porthos","card":"GFE01-005EN"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let after = state(&board);
    assert_eq!(after["objects"]["source"]["power"], 3_u8);
    assert_eq!(after["objects"]["source"]["hp"], 3_u8);
}

#[test]
fn evolved_aramis_shields_officers_for_the_turn() {
    let mut initial = on_field("GFE01-003EN");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"porthos","card":"GFE01-005EN"}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFE01-004EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"enemy","card":"BP01-037","state":{"acted":true,"hp":30_u8,"max_hp":30_u8}
    }]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 1);
    attack(&mut board, "porthos", "enemy");
    assert_eq!(state(&board)["objects"]["porthos"]["damage"], 0_u8);
}

#[test]
fn porthos_destroys_what_it_damages_and_evolved_porthos_hits_at_end() {
    let mut initial = on_field("GFE01-005EN");
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"enemy","card":"BP01-037","state":{"acted":true,"hp":30_u8,"max_hp":30_u8}
    }]);
    let mut board = game(&initial);
    attack(&mut board, "source", "enemy");
    assert_eq!(state(&board)["P2"]["cemetery"], json!(["enemy"]));
}

#[test]
fn evolved_porthos_deals_five_at_end() {
    let mut initial = evolved_on_field("GFE01-005EN", "GFE01-006EN");
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"enemy","card":"BP01-037","state":{"hp":30_u8,"max_hp":30_u8}
    }]);
    let mut board = game(&initial);
    end_phase(&mut board);
    assert_eq!(state(&board)["objects"]["enemy"]["damage"], 5_u8);
}

#[test]
fn athos_has_ward_and_evolved_athos_heals_at_end() {
    let mut board = game(&position("GFE01-007EN"));
    play(&mut board, "source");
    assert!(keyword(&state(&board), "source", "守護"));

    let mut evolved = game(&evolved_on_field("GFE01-007EN", "GFE01-008EN"));
    end_phase(&mut evolved);
    assert_eq!(state(&evolved)["P1"]["leader"]["life"], 23_u8);
}

#[test]
fn dartagnan_stores_a_swordcraft_spell_and_discounts_the_next_vow() {
    for (class, pp) in [("ロイヤル", 5_u8), ("ニュートラル", 0)] {
        let mut initial = position("GFE01-009EN");
        initial["players"]["P1"]["leader"]["class"] = json!(class);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"vow","card":"GFE01-010EN"}]);
        let mut board = game(&initial);
        play_all(&mut board, "source");
        assert_eq!(state(&board)["P1"]["ex"], json!(["vow"]), "{class}");
        play_all(&mut board, "vow");
        assert_eq!(state(&board)["P1"]["pp"]["current"], pp, "{class}");
    }
}

#[test]
fn musketeers_vow_stores_officers_and_discounts_the_musketeers() {
    let mut initial = position("GFE01-010EN");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"aramis","card":"GFE01-003EN"},
        {"id":"porthos","card":"GFE01-005EN"},
        {"id":"goblin","card":"BP01-171"}
    ]);
    let mut board = game(&initial);
    play_all(&mut board, "source");
    let stored = state(&board);
    assert_eq!(stored["P1"]["ex"], json!(["aramis", "porthos"]), "{stored}");
    play_all(&mut board, "aramis");
    assert_eq!(state(&board)["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn sky_gladiator_banishes_from_the_attack_targets_ex_area() {
    let mut initial = on_field("GFE01-011EN");
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"stored","card":"BP01-171"}]);
    let mut board = game(&initial);
    attack(&mut board, "source", "P2.leader");
    assert_eq!(state(&board)["P2"]["banish"], json!(["stored"]));
}

#[test]
fn evolved_sky_gladiator_banishes_from_each_opponents_ex_area() {
    let mut initial = evolved_on_field("GFE01-011EN", "GFE01-012EN");
    initial["players"]["P2"]["zones"]["ex"] = json!([{"id":"stored","card":"BP01-171"}]);
    let mut evolved = game(&initial);
    attack(&mut evolved, "source", "P2.leader");
    assert_eq!(state(&evolved)["P2"]["banish"], json!(["stored"]));
}

#[test]
fn vagabond_frog_draws_then_grows_and_moves_to_the_opponent_at_end() {
    let mut board = game(&position("GFE01-013EN"));
    play(&mut board, "source");
    assert_eq!(state(&board)["P1"]["hand"], json!(["draw"]));
    end_phase(&mut board);
    let after = state(&board);
    assert_eq!(after["P2"]["field"], json!(["source"]), "{after}");
    assert_eq!(after["objects"]["source"]["power"], 1_u8);
}

#[test]
fn unicorn_warrior_puts_a_unicorn_in_ex_and_grows_a_follower_when_healed() {
    let mut initial = position("GFE01-015EN");
    initial["players"]["P1"]["leader"]["class"] = json!("ビショップ");
    let mut board = game(&initial);
    play_all(&mut board, "source");
    let stored = state(&board);
    let unicorn = stored["P1"]["ex"][0].as_str().unwrap().to_owned();
    assert_eq!(stored["objects"][&unicorn]["card"], "GFE01-T01EN");
    play_all(&mut board, &unicorn);
    let after = state(&board);
    assert_eq!(
        after["P1"]["leader"]["life"], 23_u8,
        "the Unicorn heals 3: {after}"
    );
    assert_eq!(
        after["objects"]["source"]["power"], 5_u8,
        "one gain this turn"
    );
    assert!(keyword(&after, &unicorn, "守護"));
}

#[test]
fn evolved_shining_bellringer_heals_by_hand_size() {
    let mut initial = on_field("GFE01-018EN");
    initial["players"]["P1"]["zones"]["hand"] = goblins("held", 2);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFE01-019EN"}]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 1);
    assert_eq!(state(&board)["P1"]["leader"]["life"], 22_u8);
}

#[test]
fn heavens_gate_lets_each_player_summon_a_follower() {
    let mut initial = position("GFE01-020EN");
    initial["players"]["P1"]["zones"]["deck"] = goblins("deck", 3);
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"their","card":"BP01-171"}]);
    let mut board = game(&initial);
    play_all(&mut board, "source");
    let after = state(&board);
    assert_eq!(after["P1"]["field"].as_array().unwrap().len(), 2, "{after}");
    assert_eq!(after["P2"]["field"], json!(["their"]));
}

#[test]
fn heavens_gate_discards_two_to_tuck_a_follower() {
    let mut initial = on_field("GFE01-020EN");
    initial["players"]["P1"]["zones"]["hand"] = goblins("held", 2);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171"}]);
    let mut tucked = game(&initial);
    let step = tucked
        .activate_internal_card("source", activated("GFE01-020EN"), actor())
        .unwrap();
    take_all(&mut tucked, step);
    settle_all(&mut tucked);
    let moved = state(&tucked);
    assert_eq!(moved["P2"]["field"], json!([]), "{moved}");
    assert!(
        moved["P2"]["deck"]
            .as_array()
            .unwrap()
            .contains(&json!("enemy"))
    );
    assert_eq!(moved["P1"]["hand"], json!([]));
}
