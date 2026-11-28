use super::ability_combinations::finish;
use super::admission_bp_cp_fixture::{activate, actor, evolve, game, keyword, position};
use super::english_cards::state;
use super::*;

const GOBLIN: &str = "BP01-171";
const NACHT: &str = "GFE02-011EN";
const CALAMITY: &str = "GFE02-001EN";
const EVOLVED_CALAMITY: &str = "GFE02-002EN";
const BRINGER: &str = "GFE02-004EN";

fn put_on_field(number: &str) -> Value {
    let mut initial = position(number);
    let zones = &mut initial["players"]["P1"]["zones"];
    zones["field"] = zones["hand"].take();
    zones["hand"] = json!([]);
    initial
}

fn card_list(prefix: &str, count: u8, number: &str) -> Value {
    json!(
        (0..count)
            .map(|index| json!({"id":format!("{prefix}-{index}"),"card":number}))
            .collect::<Vec<_>>()
    )
}

fn reserve_form(initial: &mut Value, number: &str) {
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
}

fn play_with_modes(board: &mut Game, source: &str, modes: &[ModeId]) {
    let step = board.play_internal_card(source, actor()).unwrap();
    finish(board, Some(step), modes);
}

fn end_phase(board: &mut Game) {
    board
        .internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    finish(board, None, &[]);
}

fn pending_line(board: &mut Game, line: u64) -> CardPlay {
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed ability must be pending");
    };
    let handle = options
        .iter()
        .find(|option| option.line == line)
        .unwrap()
        .handle;
    board
        .answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
        .unwrap()
}

#[test]
fn slumbering_calamity_adds_the_third_counter_and_evolves_without_paying_pp() {
    let mut initial = put_on_field(CALAMITY);
    initial["players"]["P1"]["zones"]["field"][0]["state"]["counters"]["catastrophe_counter"] =
        json!(2_u8);
    reserve_form(&mut initial, EVOLVED_CALAMITY);
    let mut board = game(&initial);
    let pp = state(&board)["P1"]["pp"]["current"].clone();
    end_phase(&mut board);
    let after = state(&board);
    assert_eq!(
        after["objects"]["source"]["counters"]["catastrophe counter"],
        3_u8
    );
    assert_eq!(after["objects"]["source"]["evolved"], true);
    assert_eq!(after["P1"]["pp"]["current"], pp);
}

#[test]
fn unevolved_slumbering_calamity_returns_to_its_runecraft_owners_field() {
    let mut initial = position("GFE02-017EN");
    initial["players"]["P2"]["leader"]["class"] = json!("ウィッチ");
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"calamity","card":CALAMITY,"state":{"counters":{"catastrophe_counter":2_u8}}
    }]);
    let mut board = game(&initial);
    play_with_modes(&mut board, "source", &[ModeId(1)]);
    let after = state(&board);
    assert_eq!(after["P2"]["field"], json!(["calamity"]));
    assert!(after["objects"]["calamity"]["counters"]["catastrophe counter"].is_null());
}

#[test]
fn evolved_slumbering_calamity_returns_the_unevolved_card_and_turns_the_form_facedown() {
    let mut initial = position(BRINGER);
    initial["players"]["P2"]["leader"]["class"] = json!("ウィッチ");
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"calamity","card":CALAMITY,
        "state":{"evolved":true,"evolved_with":"spent","counters":{"catastrophe_counter":3_u8}}
    }]);
    initial["players"]["P2"]["zones"]["evolution"] =
        json!([{"id":"spent","card":EVOLVED_CALAMITY}]);
    let mut board = game(&initial);
    play_with_modes(&mut board, "source", &[]);
    let after = state(&board);
    assert_eq!(after["P2"]["field"], json!(["calamity"]));
    assert_eq!(after["objects"]["calamity"]["evolved"], false);
    assert!(after["objects"]["calamity"]["counters"]["catastrophe counter"].is_null());
    assert_eq!(after["P2"]["evolve_deck"], json!(["spent"]));
    assert_eq!(after["objects"]["spent"]["face_up"], false);
}

#[test]
fn evolved_slumbering_calamity_makes_the_opponent_bury_their_own_field_cards() {
    let mut initial = put_on_field(CALAMITY);
    reserve_form(&mut initial, EVOLVED_CALAMITY);
    initial["players"]["P2"]["leader"]["life"] = json!(20_u8);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"amulet","card":"GFE02-T04EN"},{"id":"follower","card":GOBLIN},
        {"id":"left","card":NACHT}
    ]);
    let mut board = seeded(&initial, "dice-3");
    board
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":10_u8}}),
        )
        .unwrap();
    let choice = question(pending_line(&mut board, 1));
    let InputDomain::Targets(domain) = &choice.domain else {
        panic!("the opponent must choose what to bury");
    };
    assert_eq!(choice.actor, PlayerId { seat: 2 });
    assert_eq!(domain.members.len(), 3);
    let selected = domain
        .members
        .iter()
        .filter(|member| matches!(member.card.as_deref(), Some(GOBLIN | "GFE02-T04EN")))
        .map(|member| member.handle)
        .collect();
    let step = board
        .answer_internal_card(choice.actor, choice.token, Answer::Targets(selected))
        .unwrap();
    finish(&mut board, Some(step), &[]);
    let after = state(&board);
    assert_eq!(after["P2"]["field"], json!(["left"]));
    let cemetery = after["P2"]["cemetery"].as_array().unwrap();
    assert_eq!(cemetery, &[json!("follower")]);
    assert_eq!(after["P2"]["deck"], json!(["their-draw"]));
    assert_eq!(after["P2"]["leader"]["life"], 15_u8);
}

#[test]
fn calamity_bringer_destroys_one_enemy_follower_and_leaves_its_amulet() {
    let mut initial = position(BRINGER);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":GOBLIN},{"id":"amulet","card":"GFE02-T04EN"}
    ]);
    let mut board = game(&initial);
    play_with_modes(&mut board, "source", &[]);
    let after = state(&board);
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(after["P2"]["field"], json!(["amulet"]));
}

#[test]
fn evolved_calamity_bringer_deals_each_accepted_roll_to_its_roller() {
    let mut initial = put_on_field(BRINGER);
    reserve_form(&mut initial, "GFE02-005EN");
    initial["players"]["P1"]["leader"]["life"] = json!(20_u8);
    initial["players"]["P2"]["leader"]["life"] = json!(20_u8);
    let mut board = seeded(&initial, "dice-3");
    evolve(&mut board, "form", 1_u8);
    let after = state(&board);
    assert_eq!(after["P1"]["leader"]["life"], 14_u8);
    assert_eq!(after["P2"]["leader"]["life"], 16_u8);
}

#[test]
fn super_evolved_calamity_bringer_rolls_in_turn_order_twice_more() {
    let mut initial = put_on_field(BRINGER);
    reserve_form(&mut initial, "GFE02-005EN");
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    let mut board = game(&initial);
    board
        .internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8,"sep":1_u8}
        }))
        .unwrap();
    let pending = board.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("both evolve abilities must be pending");
    };
    let lines = options
        .iter()
        .map(|option| option.line)
        .collect::<BTreeSet<_>>();
    assert_eq!(lines, BTreeSet::from([1, 2]));
    let mut rolls = Vec::new();
    for (line, expected_rollers) in [(1_u64, &[1, 2][..]), (2, &[1, 2, 1, 2][..])] {
        let done = resolved(pending_line(&mut board, line));
        let batch = done
            .progress
            .facts
            .iter()
            .filter_map(|fact| match fact {
                Fact::Dice(event) => Some((event.roller.seat, event.value)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            batch.iter().map(|&(seat, _)| seat).collect::<Vec<_>>(),
            expected_rollers
        );
        rolls.extend(batch);
        while board.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            board.resume_internal_card().unwrap();
        }
    }
    assert!(board.internal_pending_choice().unwrap().is_none());
    let faces = [
        DieFace::One,
        DieFace::Two,
        DieFace::Three,
        DieFace::Four,
        DieFace::Five,
        DieFace::Six,
    ];
    let after = state(&board);
    for (seat, player) in [(1, "P1"), (2, "P2")] {
        let damage = rolls
            .iter()
            .filter(|&&(roller, _)| roller == seat)
            .map(|&(_, face)| {
                faces
                    .into_iter()
                    .zip(1_u64..=6)
                    .find_map(|(value, number)| (value == face).then_some(number))
                    .unwrap()
            })
            .sum::<u64>();
        assert_eq!(
            after[player]["leader"]["life"].as_u64(),
            20_u64.checked_sub(damage)
        );
    }
}

#[test]
fn maelstrom_serpent_summons_a_demon_from_hand_and_then_one_from_cemetery() {
    let mut initial = position("GFE02-006EN");
    let zones = &mut initial["players"]["P1"]["zones"];
    zones["hand"].as_array_mut().unwrap().extend([
        json!({"id":"not-demon","card":GOBLIN}),
        json!({"id":"too-costly","card":"GFE02-006EN"}),
        json!({"id":"hand-demon","card":NACHT}),
    ]);
    zones["cemetery"] = json!([{"id":"grave-demon","card":NACHT}]);
    reserve_form(&mut initial, "GFE02-007EN");
    let mut board = game(&initial);
    play_with_modes(&mut board, "source", &[]);
    assert_eq!(
        state(&board)["P1"]["field"],
        json!(["source", "hand-demon"])
    );
    evolve(&mut board, "form", 1_u8);
    assert_eq!(
        state(&board)["P1"]["field"],
        json!(["source", "hand-demon", "grave-demon"])
    );
}

#[test]
fn lord_of_the_flies_creates_the_named_token_chosen_for_each_form() {
    let mut initial = position("GFE02-008EN");
    reserve_form(&mut initial, "GFE02-009EN");
    let mut board = game(&initial);
    play_with_modes(&mut board, "source", &[ModeId(1)]);
    let after = state(&board);
    let beetle = after["P1"]["field"][1].as_str().unwrap();
    assert_eq!(after["objects"][beetle]["card"], "GFE02-T01EN");
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
        "resolved",
    );
    finish(&mut board, None, &[ModeId(2)]);
    let after = state(&board);
    let hornet = after["P1"]["field"][2].as_str().unwrap();
    assert_eq!(after["objects"][hornet]["card"], "GFE02-T02EN");
}

#[test]
fn lord_of_the_flies_can_summon_scorpion_when_the_opponents_main_phase_starts() {
    let mut board = game(&put_on_field("GFE02-008EN"));
    assert_eq!(state(&board)["P1"]["field"], json!(["source"]));
    end_phase(&mut board);
    let after = state(&board);
    let scorpion = after["P1"]["field"][1].as_str().unwrap();
    assert_eq!(after["objects"][scorpion]["card"], "GFE02-T03EN");
}

#[test]
fn nerea_grants_other_abysscraft_followers_ward_and_weakens_the_enemy_field() {
    let number = "GFE02-010EN";
    let mut initial = position(number);
    initial["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
    let own = &mut initial["players"]["P1"]["zones"];
    own["field"] = json!([{"id":"ally","card":NACHT},{"id":"neutral","card":GOBLIN}]);
    own["cemetery"] = json!([{"id":"recover","card":NACHT}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":NACHT}]);
    let mut board = game(&initial);
    play_with_modes(&mut board, "source", &[]);
    let after = state(&board);
    assert_eq!(after["P1"]["hand"], json!(["recover"]));
    assert!(keyword(&after, "ally", "守護"));
    assert!(!keyword(&after, "source", "守護"));
    assert!(!keyword(&after, "neutral", "守護"));
    activate(&mut board, number);
    let after = state(&board);
    assert_eq!(after["objects"]["enemy"]["power"], 1_u8);
    assert_eq!(after["objects"]["enemy"]["hp"], 1_u8);
    assert_eq!(
        after["P1"]["field"],
        json!(["ally", "neutral", "source", "recover"])
    );
}

#[test]
fn nacht_activates_in_the_cemetery_and_deals_its_end_phase_damage_from_ex() {
    let mut initial = position(NACHT);
    let own = &mut initial["players"]["P1"]["zones"];
    own["cemetery"] = own["hand"].take();
    own["hand"] = json!([]);
    let mut board = game(&initial);
    let before = state(&board);
    activate(&mut board, NACHT);
    let moved = state(&board);
    assert_eq!(moved["P1"]["ex"], json!(["source"]));
    assert_eq!(
        moved["P1"]["pp"]["current"].as_u64(),
        before["P1"]["pp"]["current"]
            .as_u64()
            .unwrap()
            .checked_sub(2),
    );
    end_phase(&mut board);
    assert_eq!(
        state(&board)["P2"]["leader"]["life"].as_u64(),
        before["P2"]["leader"]["life"]
            .as_u64()
            .unwrap()
            .checked_sub(2),
    );
}

#[test]
fn baphomet_and_crow_bury_their_printed_numbers_from_the_top() {
    for (number, n) in [("GFE02-012EN", 2_u64), ("GFE02-014EN", 4)] {
        let mut initial = position(number);
        initial["players"]["P1"]["zones"]["deck"] = card_list("top", 5, GOBLIN);
        let mut board = game(&initial);
        play_with_modes(&mut board, "source", &[]);
        let after = state(&board);
        let expected = json!((0..n).map(|i| format!("top-{i}")).collect::<Vec<_>>());
        assert_eq!(after["P1"]["cemetery"], expected, "{number}");
        assert_eq!(after["P1"]["deck"][0], format!("top-{n}"), "{number}");
    }
}

#[test]
fn evolved_baphomet_orders_the_remainder_and_discounts_its_selected_ex_follower() {
    let mut initial = put_on_field("GFE02-012EN");
    reserve_form(&mut initial, "GFE02-013EN");
    let own = &mut initial["players"]["P1"]["zones"];
    own["cemetery"] = card_list("grave", 10, GOBLIN);
    own["deck"] = json!([
        {"id":"picked","card":NACHT},
        {"id":"one","card":"GFE02-016EN"},
        {"id":"two","card":"GFE02-016EN"},
        {"id":"three","card":"GFE02-016EN"},
        {"id":"four","card":"GFE02-016EN"},
        {"id":"tail","card":GOBLIN}
    ]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 1_u8);
    let stored = state(&board);
    assert_eq!(stored["P1"]["ex"], json!(["picked"]));
    assert_eq!(
        stored["P1"]["deck"],
        json!(["tail", "one", "two", "three", "four"])
    );
    play_with_modes(&mut board, "picked", &[]);
    assert_eq!(
        state(&board)["P1"]["pp"]["current"],
        stored["P1"]["pp"]["current"]
    );
}

#[test]
fn evolved_crow_summons_an_infernal_crown() {
    let mut initial = put_on_field("GFE02-014EN");
    reserve_form(&mut initial, "GFE02-015EN");
    let mut board = game(&initial);
    evolve(&mut board, "form", 2_u8);
    let after = state(&board);
    let crown = after["P1"]["field"][1].as_str().unwrap();
    assert_eq!(after["objects"][crown]["card"], "GFE02-T04EN");
}

#[test]
fn zombie_party_counts_only_followers_and_returns_the_second_target_at_twenty_cards() {
    let mut initial = position("GFE02-016EN");
    let mut grave = card_list("spell", 18, "GFE02-016EN");
    grave.as_array_mut().unwrap().extend([
        json!({"id":"return","card":NACHT}),
        json!({"id":"second","card":GOBLIN}),
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = grave;
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":NACHT}]);
    let mut board = game(&initial);
    play_with_modes(&mut board, "source", &[]);
    let after = state(&board);
    let hand = after["P1"]["hand"].as_array().unwrap();
    assert_eq!(hand.len(), 1);
    assert!(hand.contains(&json!("return")) || hand.contains(&json!("second")));
    assert_eq!(after["objects"]["enemy"]["damage"], 2_u8);
}

#[test]
fn zeus_resolves_all_three_modes_for_a_neutral_leader() {
    let mut initial = position("GFE02-017EN");
    initial["players"]["P1"]["leader"]["class"] = json!("ニュートラル");
    let enemy = &mut initial["players"]["P2"]["zones"];
    enemy["deck"] = card_list("deck", 8, GOBLIN);
    enemy["field"] = card_list("field", 2, GOBLIN);
    enemy["hand"] = card_list("hand", 2, GOBLIN);
    let mut board = game(&initial);
    play_with_modes(&mut board, "source", &[ModeId(1), ModeId(2), ModeId(3)]);
    let after = state(&board);
    assert!(after["P2"]["field"].as_array().unwrap().is_empty());
    assert!(after["P2"]["hand"].as_array().unwrap().is_empty());
    assert_eq!(after["P2"]["banish"].as_array().unwrap().len(), 7);
    assert_eq!(after["P2"]["deck"], json!(["deck-7"]));
}

#[test]
fn hamsa_recovers_three_defense_on_entry() {
    let mut initial = position("GFE02-018EN");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    let mut board = game(&initial);
    play_with_modes(&mut board, "source", &[]);
    assert_eq!(state(&board)["P1"]["leader"]["life"], 13_u8);
}

#[test]
fn apollo_hits_enemy_leader_and_followers_at_its_own_end_phase() {
    let mut initial = put_on_field("GFE02-020EN");
    initial["players"]["P2"]["leader"]["life"] = json!(20_u8);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":NACHT},{"id":"second","card":NACHT}]);
    let mut board = game(&initial);
    end_phase(&mut board);
    let after = state(&board);
    assert_eq!(after["P2"]["leader"]["life"], 19_u8);
    for id in ["enemy", "second"] {
        assert_eq!(after["objects"][id]["damage"], 1_u8);
    }
    assert_eq!(after["objects"]["source"]["damage"], 0_u8);
}

#[test]
fn each_insect_token_applies_its_own_last_words_on_destruction() {
    for (token, own_life, enemy_life, source_survives) in [
        ("GFE02-T01EN", 20_u8, 21_u8, true),
        ("GFE02-T02EN", 20, 20, false),
        ("GFE02-T03EN", 19, 20, true),
    ] {
        let mut initial = position(BRINGER);
        initial["players"]["P1"]["leader"]["life"] = json!(20_u8);
        initial["players"]["P2"]["leader"]["life"] = json!(20_u8);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"token","card":token}]);
        let mut board = game(&initial);
        play_with_modes(&mut board, "source", &[]);
        let after = state(&board);
        assert_eq!(after["P1"]["leader"]["life"], own_life, "{token}");
        assert_eq!(after["P2"]["leader"]["life"], enemy_life, "{token}");
        assert_eq!(
            after["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("source")),
            source_survives,
            "{token}"
        );
        assert!(
            after["P2"]["cemetery"].as_array().unwrap().is_empty(),
            "{token}"
        );
    }
}

#[test]
fn infernal_crown_only_buries_itself_and_summons_above_cost_five_at_necrocharge_twenty() {
    for (number, spells, field) in [
        (GOBLIN, 18, json!(["source", "target"])),
        ("GFE02-010EN", 18, json!(["source"])),
        ("GFE02-010EN", 19, json!(["target"])),
        (GOBLIN, 19, json!(["target"])),
    ] {
        let mut initial = put_on_field("GFE02-T04EN");
        let mut grave = card_list("filler", spells, "GFE02-016EN");
        grave
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"id":"target","card":number}));
        initial["players"]["P1"]["zones"]["cemetery"] = grave;
        let mut board = game(&initial);
        end_phase(&mut board);
        assert_eq!(
            state(&board)["P1"]["field"],
            field,
            "{number} with {spells} spells"
        );
    }
}
