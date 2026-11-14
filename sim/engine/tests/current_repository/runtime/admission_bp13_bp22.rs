use super::admission_bp_cp_fixture::{
    activate, destruction_game, evolve, game, keyword, next_main, play, position, settle,
};
use super::*;

#[test]
fn bp15_122_creates_an_artifact_for_each_enemy_follower() {
    let mut initial = position("BP15-122");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy-a","card":"BP01-037"},{"id":"enemy-b","card":"BP01-037"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let state = board.projection(View::Referee).unwrap();
    let tokens = state["P1"]["field"].as_array().unwrap();
    assert_eq!(tokens.len(), 2_usize);
    for token in tokens {
        let id = token.as_str().unwrap();
        assert_eq!(state["objects"][id]["card"], "BP05-T04");
        assert_eq!(state["objects"][id]["power"], 3_u8);
        assert_eq!(state["objects"][id]["hp"], 1_u8);
        assert!(keyword(&state, id, "突進"));
    }
}

#[test]
fn bp18_057_hand_play_draws_and_moves_to_ex_then_activates_there() {
    let mut initial = position("BP18-057");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["draw"]));
    assert_eq!(state["P1"]["ex"], json!(["source"]));
    activate(&mut board, "BP18-057");
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["cemetery"], json!(["source"]));
    assert_eq!(after["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(after["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn bp22_026_uses_only_face_up_evolve_followers_for_both_discounts() {
    let prints = [
        "BP01-027", "BP01-030", "BP01-035", "BP01-044", "BP02-019", "BP02-023", "BP02-026",
        "BP02-032", "BP03-021", "BP03-024",
    ];
    for (count, paid) in [(4_usize, 5_u8), (5, 2), (9, 2), (10, 0)] {
        let mut initial = position("BP22-026");
        initial["players"]["P1"]["leader"]["class"] = json!("ロイヤル");
        initial["turn"]["elapsed_turns"] = json!({"P1":12_u8,"P2":11_u8});
        let mut forms = prints
            .iter()
            .take(count)
            .enumerate()
            .map(|(i, card)| json!({"id":format!("used-{i}"),"card":card,"face_up":true}))
            .collect::<Vec<_>>();
        if count < prints.len() {
            forms.push(json!({"id":"hidden","card":prints[count],"face_up":false}));
        }
        if count <= 5 {
            forms.extend([
                json!({"id":"advance","card":"BP14-019","face_up":true}),
                json!({"id":"carrot","card":"CSD01-032","face_up":true}),
            ]);
        }
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!(forms);
        let mut board = game(&initial);
        play(&mut board, "source");
        let state = board.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], 10 - paid);
        assert!(keyword(&state, "source", "疾走"));
    }
}

#[test]
fn bp13_111_declares_two_different_enemy_roles_before_damage_and_act() {
    let mut initial = position("BP13-111");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["first"]));
    assert_eq!(state["P2"]["field"], json!(["second"]));
    assert_eq!(state["objects"]["second"]["acted"], true);
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}

#[test]
fn bp13_055_evolution_reanimates_a_martial_dragon_within_its_attack() {
    let mut initial = position("BP13-054");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP13-054"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-055"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"ally","card":"BP14-058"}]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 4);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "ally"]));
    assert_eq!(state["P1"]["cemetery"], json!([]));
}

#[test]
fn bp14_057_freezes_existing_enemies_and_its_lastword_destroys_acted_enemies() {
    let mut initial = position("BP14-056");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP14-056"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP14-057"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"frozen","card":"BP01-037","state":{"acted":true}},{"id":"standing","card":"BP01-037"}]);
    let mut board = destruction_game(&initial);
    evolve(&mut board, "form", 1);
    next_main(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["frozen"]["acted"], true);
    assert_eq!(state["objects"]["standing"]["acted"], false);
    next_main(&mut board);
    super::activate(&mut board, "P1");
    settle(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["cemetery"], json!(["frozen"]));
    assert_eq!(after["P2"]["field"], json!(["standing"]));
}

#[test]
fn bp15_102_and_bp15_103_prevent_all_attacks_only_while_standing() {
    for (evolved, acted, target) in [
        (false, false, "P1.leader"),
        (false, false, "ally"),
        (false, true, "P1.leader"),
        (false, true, "ally"),
        (true, false, "P1.leader"),
        (true, false, "ally"),
        (true, true, "P1.leader"),
        (true, true, "ally"),
    ] {
        let mut initial = position("BP15-102");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP15-102","state":{"acted":acted}},
                {"id":"ally","card":"BP01-037","state":{"acted":true}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        if evolved {
            initial["players"]["P1"]["zones"]["evolve_deck"] =
                json!([{"id":"form","card":"BP15-103"}]);
        }
        let mut board = game(&initial);
        if evolved {
            evolve(&mut board, "form", 1);
        }
        next_main(&mut board);
        let outcome = board
            .internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":target}))
            .unwrap()
            .outcome;
        assert_eq!(
            outcome,
            if acted { "resolved" } else { "cannot-attack" },
            "evolved={evolved}, acted={acted}, target={target}"
        );
    }
}

#[test]
fn bp15_103_evolution_destroys_when_marwynn_is_in_the_cemetery() {
    let mut initial = position("BP15-102");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP15-102"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-103"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"marwynn","card":"BP15-094"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 1);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}

#[test]
fn bp13_024_returns_from_the_cemetery_at_end_with_a_commander_amulet() {
    let mut initial = position("BP13-024");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"source","card":"BP13-024"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"banner","card":"BP01-033"}]);
    let mut board = game(&initial);
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["banner", "source"]));
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["P1"]["cemetery"], json!([]));
}

#[test]
fn bp13_094_observes_low_original_cost_entry_during_its_own_turn() {
    let mut initial = position("BP01-171");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"shrine","card":"BP13-094"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
    assert_eq!(state["P1"]["field"], json!(["shrine", "source"]));
}

#[test]
fn bp13_037_moves_from_hand_then_tracks_both_played_magic_card_kinds_at_end() {
    let mut initial = position("BP13-037");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-037"},{"id":"wizard","card":"BP01-059"},{"id":"spell","card":"BP01-072"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("top-{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P1"]["zones"]["cemetery"] = json!(
        (0_u8..10)
            .map(|i| json!({"id":format!("fuel-{i}"),"card":if i<5_u8 {"BP01-059"} else {"BP01-072"}}))
            .collect::<Vec<_>>()
    );
    let mut board = game(&initial);
    activate(&mut board, "BP13-037");
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["source"]));
    assert_eq!(
        state["objects"]["source"]["counters"]["魔力カウンター"],
        1_u8
    );
    play(&mut board, "wizard");
    play(&mut board, "spell");
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    settle(&mut board);
    assert_eq!(
        board.projection(View::Referee).unwrap()["objects"]["source"]["counters"]["魔力カウンター"],
        2_u8
    );
}

#[test]
fn bp13_038_recovers_distinct_spells_discounts_them_and_removes_all_counters() {
    let mut initial = position("BP13-037");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP13-037","state":{"counters":{"魔力カウンター":2_u8}}}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP13-038"}]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"wind","card":"BP01-071"},{"id":"insight","card":"BP01-072"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!(
        (0_u8..5)
            .map(|i| json!({"id":format!("top-{i}"),"card":"BP01-171"}))
            .collect::<Vec<_>>()
    );
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 4);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!(["wind", "insight"]));
    assert_eq!(
        state["objects"]["source"]["counters"]["魔力カウンター"]
            .as_u64()
            .unwrap_or(0),
        0
    );
    play(&mut board, "wind");
    play(&mut board, "insight");
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        6_u8
    );
}

#[test]
fn bp17_091_counts_amulet_pairs_and_advances_after_paying_itself() {
    let amulets = || {
        (0_u8..4)
            .map(|i| json!({"id":format!("amulet-{i}"),"card":"BP01-033"}))
            .collect::<Vec<_>>()
    };
    let mut initial = position("BP17-091");
    initial["players"]["P1"]["zones"]["field"] = json!(amulets());
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"goddess","card":"BP17-092"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy-a","card":"BP01-037"},
        {"id":"enemy-b","card":"BP01-037"},
        {"id":"enemy-c","card":"BP01-037"}
    ]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["banish"].as_array().unwrap().len(), 2_usize);
    assert_eq!(state["P2"]["field"].as_array().unwrap().len(), 1_usize);
    activate(&mut board, "BP17-091");
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["cemetery"], json!(["source"]));
    assert!(
        after["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("goddess"))
    );
    assert_eq!(after["objects"]["goddess"]["card"], "BP17-092");
    assert_eq!(after["P1"]["field"].as_array().unwrap().len(), 5_usize);
    assert_eq!(after["P1"]["pp"]["current"], 4_u8);
}
