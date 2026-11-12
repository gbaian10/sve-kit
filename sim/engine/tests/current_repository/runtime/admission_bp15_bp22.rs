use super::admission_fixture::{enter, finish, position, settle};
use super::*;

#[test]
fn bp15_pr15_whole_spell_counts_cemetery_units_and_pays_once() {
    for (graves, expected) in [(0_i32, 0), (4_i32, 0), (5_i32, 1), (9_i32, 1), (10_i32, 2)] {
        let mut initial = position("BP15-PR15");
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_i32..graves)
                .map(|i| json!({"id":format!("grave{i}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"].as_array().unwrap().len(), expected);
        assert_eq!(state["P1"]["pp"]["current"], 8_i32);
        assert_eq!(state["P1"]["hand"], json!([]));
    }
}

#[test]
fn bp16_025_requires_enemy_then_enters_an_ex_soldier_and_checks_three_names() {
    let mut initial = position("BP16-025");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"soldier","card":"BP01-T05"}]);
    let mut game = game(&initial);
    enter(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_i32);
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("soldier"))
    );
    assert_eq!(state["P1"]["ex"], json!([]));
    let ability = game.internal_activated_sources("source").unwrap()[0].definition;
    game.activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap_err();
}

#[test]
fn bp17_089_reanimates_a_low_cost_follower_and_triggers_for_its_other_class() {
    let mut initial = position("BP17-089");
    initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"ally","card":"BP01-171"}]);
    let mut game = game(&initial);
    enter(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("ally"))
    );
    assert_eq!(state["objects"]["enemy"]["damage"], 3_i32);
}

#[test]
fn bp19_072_can_damage_without_discarding_when_its_hand_is_empty() {
    let mut game = game(&position("BP19-072"));
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 6_i32);
    assert_eq!(state["P2"]["leader"]["life"], 20_i32);
}

#[test]
fn bp21_096_fanfare_requires_another_academy_follower_and_evolution_requires_hp_history() {
    let mut initial = position("BP21-096");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"evolved","card":"BP21-097"}]);
    let mut game = game(&initial);
    enter(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        10_i32
    );
    let outcome = game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"evolved","pay":{"pp":1_i32}}),
    );
    assert_eq!(outcome.unwrap().outcome, "cannot-evolve");
}

#[test]
fn bp21_097_whole_evolved_face_deals_its_current_power() {
    let mut initial = position("BP21-096");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP21-096","state":{"hp":4_i32,"max_hp":4_i32}}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"evolved","card":"BP21-097"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"booster","card":"BP21-052"}]);
    let mut game = game(&initial);
    let boost = game
        .play_internal_card("booster", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, boost);
    settle(&mut game);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"evolved","pay":{"pp":1_i32}}),
    )
    .unwrap();
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("evolution pending");
    };
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(&mut game, play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["objects"]["enemy"]["damage"],
        state["objects"]["source"]["power"]
    );
}

#[test]
fn bp17_061_zero_departure_history_keeps_printed_cost_and_damage() {
    let mut initial = position("BP17-061");
    initial["players"]["P1"]["pp"]["current"] = json!(10_i32);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 5_i32);
    assert_eq!(state["P1"]["pp"]["current"], 7_i32);
}

#[test]
fn bp19_094_reuses_actual_destroyed_amulets_for_damage_heal_and_pp() {
    let mut initial = position("BP19-093");
    initial["players"]["P1"]["pp"]["current"] = json!(7_i32);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP19-093"},{"id":"tree1","card":"BP07-T03"},{"id":"tree2","card":"BP07-T03"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP19-094"}]);
    let mut game = game(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_i32}}),
    )
    .unwrap();
    settle(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 2_i32);
    assert_eq!(state["P2"]["leader"]["life"], 18_i32);
    assert_eq!(state["P1"]["leader"]["life"], 12_i32);
    assert_eq!(state["P1"]["pp"]["current"], 8_i32);
}

#[test]
fn bp22_097_attack_draws_discards_and_uses_follower_receipt_while_preventing_combat_damage() {
    let mut initial = position("BP22-096");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fuel","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-096","state":{"evolved":true,"evolved_with":"form","power":3_i32,"hp":6_i32,"max_hp":6_i32}}]);
    initial["players"]["P1"]["zones"]["evolution"] = json!([{"id":"form","card":"BP22-097"}]);
    initial["players"]["P2"]["zones"]["field"][0]["state"]["acted"] = json!(true);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
        .unwrap();
    settle(&mut game);
    while let Some(q) = game.internal_quick_choice().unwrap() {
        let play = game
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        finish(&mut game, play);
        settle(&mut game);
    }
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(state["P2"]["leader"]["life"], 17_i32);
    assert_eq!(state["objects"]["source"]["damage"], 0_i32);
}

#[test]
fn bp17_040_discount_puts_original_cost_six_and_seven_mechs_then_reads_both_receipts() {
    let mut initial = position("BP17-040");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"six","card":"BP19-041"},{"id":"seven","card":"BP17-104"},{"id":"small","card":"BP17-041"}]);
    let mut game = game(&initial);
    enter(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 10_i32);
    assert_eq!(state["P1"]["cemetery"], json!(["six", "seven"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 4_i32);
    assert_eq!(state["objects"]["small"]["power"], 4_i32);
    assert_eq!(state["objects"]["small"]["hp"], 4_i32);
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 3);
}

#[test]
fn bp17_042_groups_machine_and_sonic_four_then_installs_only_the_super_evolve_play_observer() {
    for super_evolved in [false, true] {
        let mut initial = position("BP17-041");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP17-041"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP17-042"}]);
        initial["players"]["P1"]["sep"] = json!(1_i32);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"mech","card":"BP17-041"},{"id":"sonic","card":"BP07-041"},{"id":"bottom1","card":"BP01-171"},{"id":"bottom2","card":"BP01-171"}]);
        let mut game = game(&initial);
        let payment = if super_evolved {
            json!({"pp":1_i32,"sep":1_i32})
        } else {
            json!({"pp":1_i32})
        };
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":payment}),
        )
        .unwrap();
        for _ in 0_u8..3 {
            if game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
                game.resume_internal_card().unwrap();
            }
            let Some(q) = game.internal_pending_choice().unwrap() else {
                break;
            };
            let InputDomain::Pending(options) = &q.domain else {
                panic!("evolution pending");
            };
            let option = options.iter().find(|o| o.line == 1).unwrap_or(&options[0]);
            let trigger = game
                .answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
                .unwrap();
            if option.line == 1 {
                let CardPlay::Awaiting(choice) = trigger else {
                    panic!("grouped Look");
                };
                let InputDomain::Targets(domain) = &choice.domain else {
                    panic!("private grouped choice");
                };
                assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
                let play = game
                    .answer_internal_card(
                        choice.actor,
                        choice.token,
                        Answer::Targets(domain.members.iter().map(|m| m.handle).collect()),
                    )
                    .unwrap();
                finish(&mut game, play);
            } else {
                finish(&mut game, trigger);
            }
        }
        let looked = game.projection(View::Referee).unwrap();
        assert_eq!(looked["P1"]["hand"], json!(["mech", "sonic"]));
        assert_eq!(looked["P1"]["deck"], json!(["bottom1", "bottom2"]));
        let play = game
            .play_internal_card("mech", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play);
        settle(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            i32::from(super_evolved)
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if super_evolved { 19_i32 } else { 20_i32 }
        );
    }
}

#[test]
fn bp16_025_distinct_soldier_names_enable_exactly_one_zero_pp_activation() {
    let mut initial = position("BP16-025");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"steel","card":"BP01-T07"},{"id":"shield","card":"BP02-T02"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"knight","card":"BP01-T05"}]);
    let mut game = game(&initial);
    enter(&mut game);
    let before = game.projection(View::Referee).unwrap();
    let ability = game.internal_activated_sources("source").unwrap()[0].definition;
    let play = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 16_i32);
    assert_eq!(after["P1"]["hand"], json!(["draw1"]));
    assert_eq!(after["P1"]["pp"], before["P1"]["pp"]);
    game.activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), after);
}
