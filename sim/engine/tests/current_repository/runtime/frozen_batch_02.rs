#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate the printed scenarios."
)]
use super::*;

fn batch_selection() -> &'static SelectedEffects {
    static SELECTED: OnceLock<SelectedEffects> = OnceLock::new();
    SELECTED.get_or_init(|| {
        let (mut snapshot, keywords, mut input) = inputs().clone();
        let mut face = snapshot.lines().map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|entry| entry["number"] == "BP01-037").unwrap()["faces"][0].clone();
        face["name"] = json!("frozen counterexample opponent");
        face["text"] = Value::Null;
        face["sections"] = json!([]);
        face["power"] = json!("10");
        face["hp"] = json!("20");
        snapshot.push_str(&json!({"number":"frozen-foe","faces":[face]}).to_string());
        snapshot.push('\n');
        input["documents"].as_array_mut().unwrap().push(json!([
            "frozen-foe.yaml",
            "format: 1\nkind: effect_set\ncards:\n  frozen-foe:\n    status: complete\n    abilities: []\n"
        ]));
        Catalog::from_current_selection(&snapshot, &keywords, &input).unwrap()
    })
}

fn batch_game(value: &Value) -> Game {
    let mut game = Game::from_selected_effects(
        batch_selection().clone(),
        value,
        &Value::Null,
        &Value::Null,
        "frozen-batch",
    )
    .unwrap();
    game.load_internal_card(
        "frozen-foe",
        &WellFormedCardProgram::check(CardProgram {
            abilities: vec![],
            card_play_costs: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    game
}

fn position(card: &str, count: usize, necro: bool) -> Value {
    let mut value = draw_position();
    value["players"]["P1"]["leader"]["life"] = json!(10_u8);
    value["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    value["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-037"},{"id":"next","card":"BP01-037"},{"id":"last","card":"BP01-037"}]);
    value["players"]["P1"]["zones"]["cemetery"] = json!((0..count).map(|i| json!({"id":format!("grave-{i}"),"card":if necro { "BP01-037" } else { "BP01-065" }})).collect::<Vec<_>>());
    value["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"frozen-foe"}]);
    value
}

fn finish(game: &mut Game, mut play: CardPlay, mode: u64) -> bool {
    let mut targeted = false;
    while let CardPlay::Awaiting(question) = play {
        let answer = if let InputDomain::Targets(domain) = &question.domain {
            targeted = true;
            Answer::Targets(vec![domain.members[0].handle])
        } else if matches!(question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(mode)])
        } else if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else {
            panic!("printed target or mode")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed play has legal choices and payment"
    );
    targeted
}

fn trigger(game: &mut Game, mode: u64) -> bool {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("pending printed trigger")
    };
    assert_eq!(options.len(), 1, "one printed occurrence is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options[0].handle),
        )
        .unwrap();
    finish(game, play, mode)
}

fn play(game: &mut Game, id: &str) {
    let play = game.play_internal_card(id, PlayerId { seat: 1 }).unwrap();
    finish(game, play, 1);
}

fn fanfare(game: &mut Game) {
    play(game, "source");
    trigger(game, 1);
}

#[test]
fn bp01_070_and_alias_replace_damage_at_sc_five_and_add_leader_damage_at_ten() {
    for card in ["BP01-070", "SD03-007"] {
        for count in [4_usize, 5, 9, 10] {
            let mut game = batch_game(&position(card, count, false));
            fanfare(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["enemy"]["damage"],
                if count >= 5 { 4_u8 } else { 2_u8 }
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count >= 10 { 18_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn bp01_073_printings_always_destroy_and_only_sc_ten_damages_the_targets_leader() {
    for card in ["BP01-073", "PR-021", "SD03-014"] {
        for count in [9_usize, 10] {
            let mut game = batch_game(&position(card, count, false));
            play(&mut game, "source");
            let state = game.projection(View::Referee).unwrap();
            assert!(
                state["P2"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("enemy")),
                "destruction is unconditional"
            );
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count == 10 { 17_u8 } else { 20_u8 }
            );
        }
    }
}

#[test]
fn bp09_041_and_alias_deal_three_separate_sweeps_as_sc_ten_and_fifteen_unlock() {
    for card in ["BP09-041", "PR-316"] {
        for (count, damage) in [(9_usize, 1_u8), (10, 2), (15, 3)] {
            let mut value = position(card, count, false);
            value["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"second","card":"frozen-foe"}));
            let mut game = batch_game(&value);
            play(&mut game, "source");
            let state = game.projection(View::Referee).unwrap();
            for id in ["enemy", "second"] {
                assert_eq!(state["objects"][id]["damage"], damage);
            }
            assert_eq!(
                state["P2"]["leader"]["life"], 20_u8,
                "the sweeps only affect followers"
            );
        }
    }
}

#[test]
fn bp12_042_printings_recover_exactly_two_pp_only_at_spellchain_five() {
    for card in ["BP12-042", "BP12-P07", "EBD02-010"] {
        for count in [4_usize, 5] {
            let mut value = position(card, count, false);
            value["players"]["P1"]["pp"]["current"] = json!(8_u8);
            let mut game = batch_game(&value);
            fanfare(&mut game);
            assert_eq!(
                game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
                if count == 5 { 8_u8 } else { 6_u8 }
            );
        }
    }
}

#[test]
fn bp16_084_evolution_declares_mode_one_target_before_necrocharge_ten_damage() {
    for count in [9_usize, 10] {
        let mut value = position("BP16-084", count, true);
        value["players"]["P1"]["zones"]["hand"] = json!([]);
        value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-083"}]);
        value["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP16-084"}]);
        let mut game = batch_game(&value);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        assert!(
            trigger(&mut game, 1),
            "mode one selects its formal target even below NC ten"
        );
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if count == 10 { 5_u8 } else { 0_u8 }
        );
    }
}

#[test]
fn bp17_p32_and_t05_apply_both_damage_and_healing_only_at_spellchain_ten() {
    for card in ["BP17-P32", "BP17-T05"] {
        for count in [9_usize, 10] {
            let mut game = batch_game(&position(card, count, false));
            play(&mut game, "source");
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["P2"]["leader"]["life"],
                if count == 10 { 13_u8 } else { 20_u8 }
            );
            assert_eq!(
                state["P1"]["leader"]["life"],
                if count == 10 { 17_u8 } else { 10_u8 }
            );
        }
    }
}

#[test]
fn bp19_088_and_alias_lastword_count_the_source_cemetery_entry_before_mode_one() {
    for card in ["BP19-088", "BP19-P25"] {
        for count in [9_usize, 10] {
            let mut value = position(card, count - 1, true);
            value["turn"]["active"] = json!("P2");
            value["players"]["P1"]["zones"]["hand"] = json!([]);
            value["players"]["P1"]["zones"]["field"] =
                json!([{"id":"source","card":card,"state":{"acted":true}}]);
            let mut game = batch_game(&value);
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"enemy","target":"source"})
                )
                .unwrap()
                .outcome,
                "resolved"
            );
            let quick = game.internal_quick_choice().unwrap().unwrap();
            assert!(
                matches!(
                    game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
                        .unwrap(),
                    CardPlay::Continued(_)
                ),
                "passing Quick commits combat damage before Lastword"
            );
            let before = game.projection(View::Referee).unwrap();
            assert_eq!(before["P1"]["cemetery"].as_array().unwrap().len(), count);
            let battle_damage = before["objects"]["enemy"]["damage"].as_u64().unwrap();
            assert!(
                trigger(&mut game, 1),
                "mode one selects its formal target even below NC ten"
            );
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                battle_damage + if count == 10 { 2 } else { 0 }
            );
        }
    }
}

#[test]
fn pr_069_and_sd05_002_fanfare_add_two_attack_only_at_necrocharge_ten() {
    for card in ["PR-069", "SD05-002"] {
        for count in [9_usize, 10] {
            let mut game = batch_game(&position(card, count, true));
            fanfare(&mut game);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if count == 10 { 6_u8 } else { 4_u8 }
            );
            assert_eq!(
                state["objects"]["source"]["hp"], 4_u8,
                "the printed buff changes attack only"
            );
        }
    }
}

#[test]
fn sd03_001_replaces_three_with_five_at_sc_seven_and_adds_leader_damage_at_fifteen() {
    for count in [6_usize, 7, 15] {
        let mut game = batch_game(&position("SD03-001", count, false));
        fanfare(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if count >= 7 { 5_u8 } else { 3_u8 }
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if count >= 15 { 15_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn sd03_002_gains_four_four_at_sc_five_and_can_attack_immediately_at_sc_ten() {
    for count in [4_usize, 5, 10] {
        let mut game = batch_game(&position("SD03-002", count, false));
        fanfare(&mut game);
        let state = game.projection(View::Referee).unwrap();
        for stat in ["power", "hp"] {
            assert_eq!(
                state["objects"]["source"][stat],
                if count >= 5 { 5_u8 } else { 1_u8 }
            );
        }
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            if count == 10 {
                "resolved"
            } else {
                "cannot-attack"
            }
        );
    }
}

#[test]
fn pr_069_printings_attack_immediately_then_deal_four_and_heal_four() {
    for card in ["PR-069", "SD05-002"] {
        let mut game = batch_game(&position(card, 0, true));
        fanfare(&mut game);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        assert!(
            trigger(&mut game, 1),
            "the attack ability requires a formal follower target"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(state["P1"]["leader"]["life"], 14_u8);
        let quick = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            16_u8
        );
    }
}

#[test]
fn bp16_084_mode_two_mills_two_without_a_formal_follower_target() {
    let mut value = position("BP16-084", 9, true);
    value["players"]["P1"]["zones"]["hand"] = json!([]);
    value["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP16-083"}]);
    value["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP16-084"}]);
    let mut game = batch_game(&value);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    assert!(
        !trigger(&mut game, 2),
        "mode two has no formal follower target"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 11);
    assert_eq!(state["P1"]["deck"], json!(["last"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 0_u8);
}

#[test]
fn bp19_088_printings_mode_two_mills_one_without_a_formal_follower_target() {
    for card in ["BP19-088", "BP19-P25"] {
        let mut value = position(card, 8, true);
        value["turn"]["active"] = json!("P2");
        value["players"]["P1"]["zones"]["hand"] = json!([]);
        value["players"]["P1"]["zones"]["field"] =
            json!([{"id":"source","card":card,"state":{"acted":true}}]);
        let mut game = batch_game(&value);
        game.internal_rule_action(&json!({"do":"attack","attacker":"enemy","target":"source"}))
            .unwrap();
        let quick = game.internal_quick_choice().unwrap().unwrap();
        game.answer_internal_card(quick.actor, quick.token, Answer::Quick(None))
            .unwrap();
        let before = game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"].clone();
        assert!(
            !trigger(&mut game, 2),
            "mode two has no formal follower target"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 10);
        assert_eq!(state["P1"]["deck"], json!(["next", "last"]));
        assert_eq!(state["objects"]["enemy"]["damage"], before);
    }
}
