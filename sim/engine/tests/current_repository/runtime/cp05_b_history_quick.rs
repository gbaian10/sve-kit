
use super::*;

const GOLD: &str = "CP05-T15";
const FINAL_BEARER: &str = "CP05-110";
const ENEMY: &str = "BP01-037";

fn settle(game: &mut Game, mut play: Option<CardPlay>) {
    loop {
        while let Some(CardPlay::Awaiting(question)) = play {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => Answer::Targets(
                    domain
                        .members
                        .iter()
                        .take(1)
                        .map(|member| member.handle)
                        .collect(),
                ),
                InputDomain::CostMaterials(domain) => Answer::CostMaterials(
                    domain.members.iter().map(|member| member.handle).collect(),
                ),
                InputDomain::Search(domain) => Answer::Search(SearchAnswer::Ungrouped(
                    domain
                        .cards
                        .iter()
                        .take(1)
                        .map(|card| card.handle)
                        .collect(),
                )),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                _ => panic!("unexpected printed-card choice: {:?}", question.domain),
            };
            play = Some(
                game.answer_internal_card(question.actor, question.token, answer)
                    .unwrap(),
            );
        }
        finish_checks(game);
        let Some(question) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending printed ability");
        };
        play = Some(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options.first().unwrap().handle),
            )
            .unwrap(),
        );
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    settle(game, Some(step));
}

fn enemy_position(card: &str, zone: &str) -> Value {
    let mut initial = position_with_source(card, zone);
    initial["turn"]["elapsed_turns"] = json!({"P1":5_u8,"P2":4_u8});
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":ENEMY,"state":{"hp":20_u8,"max_hp":20_u8}}]);
    initial
}

fn enemy_hp(game: &Game) -> Value {
    state(game)["objects"]["enemy"]["hp"].clone()
}

fn enemy_life(game: &Game) -> Value {
    state(game)["P2"]["leader"]["life"].clone()
}

fn has_card(game: &Game, zone: &str, printing: &str) -> bool {
    let board = state(game);
    board["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| board["objects"][id.as_str().unwrap()]["card"] == printing)
}

fn activate(game: &mut Game, holder: &str, printing: &str) -> CardPlay {
    game.activate_internal_card(holder, activated(printing), actor())
        .unwrap()
}

fn history_position(card: &str, zone: &str) -> Value {
    let mut initial = enemy_position(card, zone);
    let zones = &mut initial["players"]["P1"]["zones"];
    let mut field = zones["field"].as_array().cloned().unwrap_or_default();
    field.extend([
        json!({"id":"final-bearer","card":FINAL_BEARER}),
        json!({"id":"bearer","card":"CP05-003"}),
    ]);
    zones["field"] = json!(field);
    zones["ex"] = json!([{"id":"first-gold","card":GOLD},{"id":"second-gold","card":GOLD}]);
    initial
}

fn break_with(game: &mut Game, material: &str, bearer: &str) -> CardPlay {
    let step = activate(game, material, GOLD);
    choose_card_by_number(game, step, bearer)
}

fn break_history(game: &mut Game, third: bool) {
    let first = break_with(game, "first-gold", FINAL_BEARER);
    settle(game, Some(first));
    if third {
        let second = break_with(game, "second-gold", "CP05-003");
        settle(game, Some(second));
    }
}

fn end_phase(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the turn owner enters the end phase",
    );
    settle(game, None);
}

fn evolve(game: &mut Game) {
    assert_eq!(
        game.internal_rule_action(&json!({
            "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}
        }))
        .unwrap()
        .outcome,
        "resolved",
        "the actual base evolves for one PP",
    );
    settle(game, None);
}

fn quick_cemetery(initial: &mut Value, count: usize) {
    let printings = [
        "CP05-039", "CP05-039", "CP05-039", "CP05-043", "CP05-043", "CP05-043", "CP05-009",
    ];
    let mut cards = vec![json!({"id":"ordinary","card":"CP05-003"})];
    cards.extend(
        printings
            .into_iter()
            .take(count)
            .enumerate()
            .map(|(index, card)| json!({"id":format!("quick-{index}"),"card":card})),
    );
    initial["players"]["P1"]["zones"]["cemetery"] = json!(cards);
}

#[test]
fn yuel_and_societte_reveal_exactly_three_fire_cards_for_the_fanfare() {
    let mut initial = enemy_position("CP05-002", "hand");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-002"},
        {"id":"fire-a","card":"CP05-025"},{"id":"fire-b","card":"CP05-025"},
        {"id":"fire-c","card":"CP05-025"},{"id":"wrong","card":"CP05-003"}
    ]);
    let mut game = game(&initial);
    let payment = enter_with_fanfare(&mut game, "CP05-002");
    let question = await_card_choice(choose_card_by_number(&mut game, payment, ENEMY));
    let InputDomain::CostMaterials(domain) = &question.domain else {
        panic!("three private fire materials");
    };
    assert_eq!(domain.members.len(), 3);
    assert_eq!(domain.cardinality.minimum, Nat(3));
    assert!(game.internal_decision(View::P2).is_none());
    let handles = domain.members.iter().map(|member| member.handle).collect();
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::CostMaterials(handles),
        )
        .unwrap(),
    );
    assert_eq!(enemy_hp(&game), 17_u8);
    assert_eq!(enemy_life(&game), 17_u8);
    assert_eq!(
        state(&game)["P1"]["hand"],
        json!(["fire-a", "fire-b", "fire-c", "wrong"])
    );
}

#[test]
fn yuel_and_societte_need_another_successful_quick_activation() {
    let mut initial = enemy_position("CP05-002", "hand");
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP05-002"},{"id":"starter","card":"CP05-007"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(
        game.activate_internal_card("source", activated("CP05-002"), actor()),
        Err(EngineFailure::IllegalPlay(message)) if message.contains("use prerequisite")
    ));
    let starter = activate(&mut game, "starter", "CP05-007");
    settle(&mut game, Some(starter));
    assert_eq!(enemy_hp(&game), 16_u8);
    pass_quick(&mut game);
    let gated = activate(&mut game, "source", "CP05-002");
    settle(&mut game, Some(gated));
    assert_eq!(enemy_hp(&game), 12_u8);
    assert_eq!(pp(&game), 7);
    assert_eq!(state(&game)["P1"]["cemetery"], json!(["starter", "source"]));
}

#[test]
fn korwa_puts_wind_in_ex_and_creates_fil_only_at_three_stages() {
    for third in [false, true] {
        let mut game = game(&history_position("CP05-005", "hand"));
        break_history(&mut game, third);
        play(&mut game);
        assert!(has_card(&game, "ex", "CP05-T12"));
        end_phase(&mut game);
        assert_eq!(has_card(&game, "ex", "CP05-T02"), third);
    }
}

#[test]
fn evolved_korwa_searches_a_one_cost_wind_card_and_creates_fil() {
    let mut initial = history_position("CP05-005", "field");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-006"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wrong-cost","card":"CP05-011"},{"id":"wrong-trait","card":"CP05-015"},
        {"id":"matching","card":"CP05-003"}
    ]);
    let mut game = game(&initial);
    break_history(&mut game, true);
    evolve(&mut game);
    assert_eq!(state(&game)["P1"]["hand"], json!(["matching"]));
    end_phase(&mut game);
    assert!(has_card(&game, "ex", "CP05-T02"));
}

#[test]
fn silva_revives_only_quick_followers_with_a_combined_original_cost_of_six() {
    let mut initial = enemy_position("CP05-007", "field");
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"gold","card":GOLD}]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"first","card":"CP05-008"},{"id":"second","card":"CP05-043"},
        {"id":"expensive","card":"CP05-030"},{"id":"plain","card":"CP05-003"}
    ]);
    let mut game = game(&initial);
    resolved(break_with(&mut game, "gold", "CP05-007"));
    finish_checks(&mut game);
    let question = await_card_choice(resolve_pending_card(&mut game, "CP05-007"));
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("public Quick follower targets");
    };
    assert_eq!(domain.members.len(), 3);
    let handle = |number| {
        domain
            .members
            .iter()
            .find(|member| member.card.as_deref() == Some(number))
            .unwrap()
            .handle
    };
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Targets(vec![handle("CP05-008"), handle("CP05-030")]),
    )
    .unwrap_err();
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![handle("CP05-008"), handle("CP05-043")]),
        )
        .unwrap(),
    );
    assert!(has_card(&game, "field", "CP05-008"));
    assert!(has_card(&game, "field", "CP05-043"));
    assert_eq!(
        state(&game)["P1"]["cemetery"],
        json!(["expensive", "plain"])
    );
}

#[test]
fn cupitan_only_increases_damage_and_hits_the_leader_at_seven_quick_followers() {
    for count in [6_usize, 7_usize] {
        let mut initial = enemy_position("CP05-008", "field");
        quick_cemetery(&mut initial, count);
        let mut game = game(&initial);
        let step = activate(&mut game, "source", "CP05-008");
        settle(&mut game, Some(step));
        assert_eq!(enemy_hp(&game), if count == 7 { 13_u8 } else { 19_u8 });
        assert_eq!(enemy_life(&game), if count == 7 { 18_u8 } else { 20_u8 });
        assert_eq!(state(&game)["objects"]["source"]["acted"], true);
    }
}

#[test]
fn cucouroux_creates_cumuyu_only_at_seven_quick_followers() {
    for count in [6_usize, 7_usize] {
        let mut initial = enemy_position("CP05-009", "hand");
        quick_cemetery(&mut initial, count);
        let mut game = game(&initial);
        play(&mut game);
        assert_eq!(has_card(&game, "field", "CP05-T01"), count == 7);
    }
}

#[test]
fn metera_deals_five_to_a_follower_on_break_and_the_leader_at_end() {
    let mut game = game(&history_position("CP05-011", "field"));
    break_history(&mut game, false);
    let own_break = break_with(&mut game, "second-gold", "CP05-011");
    settle(&mut game, Some(own_break));
    assert_eq!(enemy_hp(&game), 15_u8);
    end_phase(&mut game);
    assert_eq!(enemy_life(&game), 15_u8);
}

#[test]
fn shooting_star_subtracts_both_stages_and_hits_the_target() {
    let mut game = game(&history_position("CP05-018", "hand"));
    break_history(&mut game, false);
    play(&mut game);
    assert_eq!(pp(&game), 9);
    assert_eq!(enemy_hp(&game), 16_u8);
    assert!(has_card(&game, "ex", "CP05-T12"));
}

#[test]
fn mirin_damages_the_enemy_leader_at_three_successful_stages() {
    for third in [false, true] {
        let mut game = game(&history_position("CP05-027", "field"));
        break_history(&mut game, third);
        end_phase(&mut game);
        assert_eq!(enemy_life(&game), if third { 18_u8 } else { 20_u8 });
    }
}

#[test]
fn evolved_mirin_deals_four_after_her_own_break_and_two_without_it() {
    for break_source in [false, true] {
        let mut initial = history_position("CP05-027", "field");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-028"}]);
        let mut game = game(&initial);
        break_history(&mut game, false);
        if break_source {
            let own_break = break_with(&mut game, "second-gold", "CP05-027");
            settle(&mut game, Some(own_break));
        }
        evolve(&mut game);
        assert_eq!(enemy_hp(&game), if break_source { 16_u8 } else { 18_u8 });
        end_phase(&mut game);
        assert_eq!(enemy_life(&game), if break_source { 18_u8 } else { 20_u8 });
    }
}

#[test]
fn aglovale_counts_only_quick_followers_and_hits_every_enemy_follower() {
    let mut initial = enemy_position("CP05-030", "hand");
    quick_cemetery(&mut initial, 3);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"other","card":ENEMY,"state":{"hp":20_u8,"max_hp":20_u8}}));
    let mut game = game(&initial);
    play(&mut game);
    assert_eq!(enemy_hp(&game), 17_u8);
    assert_eq!(state(&game)["objects"]["other"]["hp"], 17_u8);
    assert_eq!(enemy_life(&game), 20_u8);
}

fn knight_position(card: &str, partner: &str) -> Value {
    let mut initial = history_position(card, "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"partner","card":partner}]);
    initial
}

#[test]
fn arthur_finds_mordred_and_gains_three_keywords_at_three_stages() {
    let mut game = game(&knight_position("CP05-033", "CP05-034"));
    break_history(&mut game, true);
    play(&mut game);
    assert!(has_card(&game, "field", "CP05-034"));
    for keyword in ["突進", "指定攻撃", "必殺"] {
        assert!(has_granted_keyword(&state(&game), "source", keyword));
    }
}

#[test]
fn mordred_finds_arthur_and_only_gains_storm_at_three_stages() {
    for third in [false, true] {
        let mut game = game(&knight_position("CP05-034", "CP05-033"));
        break_history(&mut game, third);
        play(&mut game);
        assert!(has_card(&game, "field", "CP05-033"));
        assert_eq!(has_granted_keyword(&state(&game), "source", "疾走"), third);
    }
}

#[test]
fn randall_costs_one_after_three_stages_and_returns_at_end() {
    let mut game = game(&history_position("CP05-036", "hand"));
    break_history(&mut game, true);
    play(&mut game);
    assert_eq!(pp(&game), 9);
    assert!(has_granted_keyword(&state(&game), "source", "疾走"));
    end_phase(&mut game);
    assert!(has_card(&game, "hand", "CP05-036"));
}

#[test]
fn evolved_mirin_remembers_whether_she_limit_broke_when_another_trigger_returns_her() {
    let mut tree = expanded::tree().clone();
    let mut row = tree["cards"]["BP01-041"].clone();
    let mut entry = row["abilities"][0].clone();
    entry.as_object_mut().unwrap().remove("activate");
    entry["auto"] = json!({
        "on":{"evolved":{"who":"self","sel":{"kind":"follower"}}},
        "targets":[{"bind":"returned","from":{"who":"self","zone":"field","kind":"follower"},"n":1_u8}],
        "do":[{"move":{"ref":"returned","to":"hand"}}]
    });
    row["abilities"] = json!([entry]);
    tree["cards"] = json!({"producer":row});
    let bounce = lowerer().lower(&tree).unwrap().remove("producer").unwrap();

    for break_source in [false, true] {
        let mut initial = enemy_position("CP05-027", "field");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP05-028"}]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"gold","card":GOLD}]);
        let mut game = with_selected_producer_program(&initial, &bounce, producer_selection());
        if break_source {
            let own_break = break_with(&mut game, "gold", "CP05-027");
            settle(&mut game, Some(own_break));
        }

        assert_eq!(
            game.internal_rule_action(&json!({
                "do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}
            }))
            .unwrap()
            .outcome,
            "resolved"
        );
        finish_checks(&mut game);
        let first = resolve_pending_card(&mut game, "producer");
        resolved(choose_card_by_number(&mut game, first, "CP05-027"));
        finish_checks(&mut game);
        assert_eq!(state(&game)["P1"]["hand"], json!(["source"]));

        let mirin = resolve_pending_card(&mut game, "CP05-028");
        resolved(choose_card_by_number(&mut game, mirin, ENEMY));
        assert_eq!(enemy_hp(&game), if break_source { 16_u8 } else { 18_u8 });
    }
}
