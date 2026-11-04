#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed equipment scenarios."
)]
use super::event_cemetery::finish_checks;
use super::union_burst_batch1::finish;
use super::*;

fn settle(board: &mut Game) {
    for _ in 0_u8..12 {
        finish_checks(board);
        let Some(question) = board.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("expected printed trigger");
        };
        let play = board
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        resolved(finish(board, play));
    }
    panic!("printed trigger loop");
}
fn actual(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "equipment-light2",
    )
    .unwrap()
}
fn position() -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["construction"] = json!("title");
    initial["players"]["P1"]["title_code"] = json!("princess_connect_re_dive");
    initial["turn"]["elapsed_turns"]["P1"] = json!(10_u8);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!(
            (0_u8..5)
                .map(|n| json!({"id":format!("{seat}draw{n}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
    }
    initial
}
fn combat(board: &mut Game) {
    settle(board);
    while let Some(q) = board.internal_quick_choice().unwrap() {
        board
            .answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
        settle(board);
    }
}
fn equip_card(after: &Value) -> &str {
    let id = after["P1"]["equipment"][0].as_str().unwrap();
    assert_eq!(
        after["objects"][id]["equipped_to"], "host",
        "the real equipment belongs to its printed host"
    );
    id
}
#[test]
fn pecorine_all_printings_pay_for_princess_sword_and_increase_combat_damage() {
    for card in ["CP04-019", "CP04-SL05", "PCS01-008"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"host","card":card}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":10_u8,"max_hp":10_u8}}]);
        let mut board = actual(&initial);
        let play = board
            .play_internal_card("host", PlayerId { seat: 1 })
            .unwrap();
        assert!(matches!(finish(&mut board, play), CardPlay::Entered(_)));
        settle(&mut board);
        let equipped = board.projection(View::Referee).unwrap();
        assert_eq!(equipped["P1"]["pp"]["current"], 5_u8);
        equip_card(&equipped);
        attack_batch::end_turn(&mut board);
        attack_batch::end_turn(&mut board);
        assert_eq!(
            board
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"host","target":"P2.leader"})
                )
                .unwrap()
                .outcome,
            "resolved"
        );
        combat(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            15_u8
        );
    }
}
#[test]
fn muimi_all_printings_draw_discard_equip_and_buff_from_maximum_pp() {
    for card in ["CP04-058", "CP04-SL16", "CP04-SP08", "CP04-U08", "PR-503"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"host","card":card},{"id":"fee","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}},{"id":"enemy2","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
        let mut board = actual(&initial);
        assert!(matches!(
            board
                .play_internal_card("host", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        settle(&mut board);
        let equipped = board.projection(View::Referee).unwrap();
        assert_eq!(equipped["P1"]["pp"]["current"], 8_u8);
        assert_eq!(equipped["P1"]["hand"], json!(["P1draw0"]));
        assert_eq!(equipped["P1"]["cemetery"], json!(["fee"]));
        assert_eq!(equipped["objects"]["host"]["power"], 5_u8);
        equip_card(&equipped);
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"host","target":"P2.leader"}))
            .unwrap();
        combat(&mut board);
        let after = board.projection(View::Referee).unwrap();
        for enemy in ["enemy", "enemy2"] {
            assert_eq!(after["objects"][enemy]["damage"], 3_u8);
        }
        assert_eq!(after["P2"]["leader"]["life"], 15_u8);
    }
}
#[test]
fn christina_all_printings_equip_and_repeat_attack_but_avalon_triggers_once() {
    for card in ["CP04-021", "CP04-SL07", "CP04-SP04", "CP04-U04"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] =
            json!([{"id":"host","card":card},{"id":"fee","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
        let mut board = actual(&initial);
        assert!(matches!(
            board
                .play_internal_card("host", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        settle(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            3_u8
        );
        for (damage, acted) in [(5_u8, false), (10_u8, true)] {
            board
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"host","target":"P2.leader"}),
                )
                .unwrap();
            combat(&mut board);
            let after = board.projection(View::Referee).unwrap();
            equip_card(&after);
            assert_eq!(after["objects"]["enemy"]["damage"], damage);
            assert_eq!(after["objects"]["host"]["acted"], acted);
        }
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["leader"]["life"], 12_u8);
        assert_eq!(after["P1"]["deck"].as_array().unwrap().len(), 4);
        assert_eq!(after["P1"]["cemetery"].as_array().unwrap().len(), 2);
    }
}
#[test]
fn avalon_all_printings_grant_exactly_one_stand_and_draw_per_turn() {
    for card in ["CP04-T03", "CP04-PR17", "CP04-PR29"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"host","card":"BP01-037"}]);
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":card,"state":{"equipped_to":"host"}}]);
        let mut board = actual(&initial);
        for acted in [false, true] {
            board
                .internal_rule_action(
                    &json!({"do":"attack","attacker":"host","target":"P2.leader"}),
                )
                .unwrap();
            combat(&mut board);
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["host"]["acted"],
                acted
            );
        }
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["hand"],
            json!(["P1draw0"])
        );
    }
}
#[test]
fn bond_all_printings_grant_storm_and_damage_two_chosen_followers() {
    for card in ["CP04-T09", "CP04-PR23", "CP04-PR35"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"host","card":"CP04-058","state":{"entered_this_turn":true}}]);
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":card,"state":{"equipped_to":"host"}}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}},{"id":"enemy2","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
        let mut board = actual(&initial);
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"host","target":"P2.leader"}))
            .unwrap();
        combat(&mut board);
        let after = board.projection(View::Referee).unwrap();
        for enemy in ["enemy", "enemy2"] {
            assert_eq!(after["objects"][enemy]["damage"], 3_u8);
        }
        assert_eq!(after["P2"]["leader"]["life"], 18_u8);
    }
}
#[test]
fn feather_all_printings_increase_outgoing_damage_and_heal_at_own_end() {
    for card in ["CP04-T12", "CP04-PR26", "CP04-PR38"] {
        let mut initial = position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"host","card":"BP01-037","state":{"power":5_u8}}]);
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":card,"state":{"equipped_to":"host"}}]);
        let mut board = actual(&initial);
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"host","target":"P2.leader"}))
            .unwrap();
        combat(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            14_u8
        );
        board
            .internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap();
        combat(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            13_u8
        );
        let mut ability_initial = position();
        ability_initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"host","card":"CP04-040"}]);
        ability_initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":card,"state":{"equipped_to":"host"}}]);
        ability_initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":10_u8,"max_hp":10_u8}}]);
        let mut ability_board = actual(&ability_initial);
        fire(&mut ability_board, "host");
        assert_eq!(
            ability_board.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_u8
        );
    }
}
#[test]
fn specter_both_printings_pay_life_mill_and_gain_rush_for_a_bane_attack() {
    for card in ["SD05-007", "BP01-119"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["class"] = json!("ナイトメア");
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"host","card":card}]);
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"milled","card":"BP01-037"},{"id":"spare","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037","state":{"acted":true,"hp":10_u8,"max_hp":10_u8}}]);
        let mut board = actual(&initial);
        assert!(matches!(
            board
                .play_internal_card("host", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        settle(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 18_u8);
        assert_eq!(after["P1"]["cemetery"], json!(["milled"]));
        board
            .internal_rule_action(&json!({"do":"attack","attacker":"host","target":"enemy"}))
            .unwrap();
        combat(&mut board);
        assert_eq!(
            board.projection(View::Referee).unwrap()["P2"]["cemetery"],
            json!(["enemy"])
        );
    }
}

fn fire(board: &mut Game, holder: &str) {
    let sources = board.internal_activated_sources(holder).unwrap();
    assert_eq!(
        sources.len(),
        1,
        "each printed mirror ability has one ordinary activation"
    );
    let play = board
        .activate_internal_card_instance(holder, sources[0].instance, PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(board, play));
    settle(board);
}
#[test]
fn neneka_all_printings_create_one_mirror_equip_and_increase_both_ability_sources() {
    for card in ["CP04-040", "CP04-SL12", "PR-502"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"host","card":card}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
        let mut board = actual(&initial);
        assert!(matches!(
            board
                .play_internal_card("host", PlayerId { seat: 1 })
                .unwrap(),
            CardPlay::Entered(_)
        ));
        settle(&mut board);
        let after = board.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["pp"]["current"], 6_u8);
        equip_card(&after);
        let field = after["P1"]["field"].as_array().unwrap();
        assert_eq!(field.len(), 2);
        let mirror = field
            .iter()
            .filter_map(Value::as_str)
            .find(|id| *id != "host")
            .unwrap()
            .to_owned();
        fire(&mut board, "host");
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            2_u8
        );
        fire(&mut board, &mirror);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            4_u8
        );
    }
}
#[test]
fn wand_all_printings_count_an_equipped_mirror_once_and_exclude_other_names() {
    for card in ["CP04-T07", "CP04-PR21", "CP04-PR33"] {
        let mut initial = position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"host","card":"CP04-T05"},{"id":"mirror","card":"CP04-T05"},{"id":"neneka","card":"CP04-040"}]);
        initial["players"]["P1"]["zones"]["equipment"] =
            json!([{"id":"equipment","card":card,"state":{"equipped_to":"host"}}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
        let mut ordinary = initial.clone();
        ordinary["players"]["P1"]["zones"]["field"][0]["state"]["power"] = json!(1_u8);
        let mut combat_board = actual(&ordinary);
        combat_board
            .internal_rule_action(&json!({"do":"attack","attacker":"host","target":"P2.leader"}))
            .unwrap();
        combat(&mut combat_board);
        assert_eq!(
            combat_board.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            19_u8
        );
        let mut board = actual(&initial);
        for (source, damage) in [("host", 2_u8), ("mirror", 4), ("neneka", 5)] {
            fire(&mut board, source);
            assert_eq!(
                board.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                damage
            );
        }
    }
}

#[test]
fn q2783_two_wand_hosts_each_increase_the_same_mirrors_ability_damage() {
    let mut initial = position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"first","card":"CP04-040"},{"id":"second","card":"CP04-040"},{"id":"mirror","card":"CP04-T05"}]);
    initial["players"]["P1"]["zones"]["equipment"] = json!([{"id":"wand1","card":"CP04-T07","state":{"equipped_to":"first"}},{"id":"wand2","card":"CP04-PR21","state":{"equipped_to":"second"}}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8,"max_hp":20_u8}}]);
    let mut board = actual(&initial);
    for (source, damage) in [("mirror", 3_u8), ("first", 5), ("second", 7)] {
        fire(&mut board, source);
        assert_eq!(
            board.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            damage
        );
    }
}
