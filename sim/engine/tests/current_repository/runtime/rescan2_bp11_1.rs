#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-171"},
        {"id":"second","card":"BP01-171"},
        {"id":"third","card":"BP01-171"},
        {"id":"fourth","card":"BP01-171"}
    ]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(&question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::Search(domain) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                domain
                    .cards
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|card| card.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &question.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if let InputDomain::CostMaterials(domain) = &question.domain {
            Answer::CostMaterials(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if let InputDomain::Targets(domain) = &question.domain {
            Answer::Targets(
                domain
                    .members
                    .iter()
                    .take(usize::try_from(domain.cardinality.maximum.unwrap().0).unwrap())
                    .map(|member| member.handle)
                    .collect(),
            )
        } else {
            panic!("unexpected decision")
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
}

fn play(game: &mut Game) {
    let step = game.play_internal_card("source", actor()).unwrap();
    finish(game, step);
}

fn triggers(game: &mut Game) {
    while let Some(question) = game.internal_pending_choice().unwrap() {
        let InputDomain::Pending(options) = &question.domain else {
            panic!("pending ability expected")
        };
        let step = game
            .answer_internal_card(
                question.actor,
                question.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap();
        finish(game, step);
        game.resume_internal_card().unwrap();
    }
}

fn pass_quick(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(game, step);
}

#[test]
fn bp11_028_fallback_damages_follower_and_leader_without_both_named_allies() {
    for bunny in [false, true] {
        let mut initial = position("BP11-028");
        if bunny {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"bunny","card":"BP11-019"}]);
        }
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    }
}

#[test]
fn bp11_084_crimson_replaces_one_leader_damage_with_three() {
    for crimson in [false, true] {
        let mut initial = position("BP11-084");
        if crimson {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"payer","card":"BP01-118"}));
        }
        let mut game = game(&initial);
        if crimson {
            let step = game.play_internal_card("payer", actor()).unwrap();
            finish(&mut game, step);
            triggers(&mut game);
        }
        play(&mut game);
        triggers(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if crimson { 16_u8 } else { 19_u8 }
        );
    }
}

#[test]
fn bp11_016_combo_three_replaces_three_damage_with_five() {
    for prior in [0_usize, 2] {
        let mut initial = position("BP11-016");
        for n in 0..prior {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        for n in 0..prior {
            let step = game
                .play_internal_card(&format!("prior{n}"), actor())
                .unwrap();
            finish(&mut game, step);
            game.resume_internal_card().unwrap();
        }
        play(&mut game);
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["field"],
            if prior == 0 {
                json!(["enemy"])
            } else {
                json!([])
            }
        );
    }
}

#[test]
fn bp11_055_fanfare_damage_requires_awakened_pp_maximum() {
    for awakened in [false, true] {
        let mut initial = position("BP11-055");
        initial["players"]["P1"]["pp"] = if awakened {
            json!({"current":10_u8,"max":10_u8})
        } else {
            json!({"current":6_u8,"max":6_u8})
        };
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if awakened { 2_u8 } else { 0_u8 }
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if awakened { 18_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn bp11_051_adds_leader_damage_only_with_two_wizards() {
    for two in [false, true] {
        let mut initial = position("BP11-051");
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"wizard","card":"BP02-046"},
            {"id":"other","card":if two {"BP08-042"} else {"BP01-171"}}
        ]);
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(
            state["P2"]["leader"]["life"],
            if two { 18_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn bp11_049_taking_combat_damage_during_opponents_turn_hits_their_leader() {
    let mut initial = position("BP11-049");
    initial["turn"]["active"] = json!("P2");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"source","card":"BP11-049","state":{"acted":true}}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"attacker","card":"BP01-171"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"attacker","target":"source"}))
            .unwrap()
            .outcome,
        "resolved",
        "the opponent legally attacks the rested witch"
    );
    pass_quick(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["damage"], 2_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn bp11_085_acts_enemy_draws_and_blocks_its_next_rule_stand() {
    let mut initial = position("BP11-085");
    initial["players"]["P2"]["zones"]["deck"] = json!([{ "id":"opp-draw", "card":"BP01-171" }]);
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    let before_turn = game.projection(View::Referee).unwrap();
    assert_eq!(before_turn["objects"]["enemy"]["acted"], true);
    assert_eq!(before_turn["P1"]["hand"], json!(["first"]));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the current player can end the turn"
    );
    pass_quick(&mut game);
    let after_turn = game.projection(View::Referee).unwrap();
    assert_eq!(after_turn["turn"]["active"], "P2");
    assert_eq!(after_turn["objects"]["enemy"]["acted"], true);
}

#[test]
fn bp11_100_fanfare_checks_amulet_and_attack_draws_before_discarding() {
    for amulet in [false, true] {
        let mut initial = position("BP11-100");
        if amulet {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-016"}]);
        }
        let mut played = game(&initial);
        play(&mut played);
        triggers(&mut played);
        let state = played.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if amulet { 2_u8 } else { 1_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["hp"],
            if amulet { 3_u8 } else { 2_u8 }
        );
    }
    let mut initial = position("BP11-100");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP11-100"}]);
    let mut attacking = game(&initial);
    assert_eq!(
        attacking
            .internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the standing follower legally attacks"
    );
    triggers(&mut attacking);
    let after_attack = attacking.projection(View::Referee).unwrap();
    assert_eq!(after_attack["P1"]["hand"], json!([]));
    assert_eq!(after_attack["P1"]["cemetery"], json!(["first"]));
    assert_eq!(
        after_attack["P1"]["deck"],
        json!(["second", "third", "fourth"])
    );
}

#[test]
fn bp11_080_amulet_grants_entry_turn_rush_and_assail() {
    for amulet in [false, true] {
        let mut initial = position("BP11-080");
        if amulet {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-016"}]);
        }
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "cannot-attack"
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            if amulet { "resolved" } else { "cannot-attack" }
        );
    }
}

#[test]
fn bp11_107_finds_goblin_then_pays_three_grave_goblins_to_destroy() {
    let mut initial = position("BP11-107");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"wrong","card":"BP01-037"},{"id":"goblin","card":"BP01-171"},
        {"id":"third","card":"BP01-037"},{"id":"fourth","card":"BP01-037"},
        {"id":"fifth","card":"BP01-037"},{"id":"tail","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["cemetery"] = json!([
        {"id":"a","card":"BP01-171"},{"id":"b","card":"BP01-171"},
        {"id":"c","card":"BP01-171"},{"id":"other","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["hand"], json!(["goblin"]));
    assert_eq!(
        before["P1"]["deck"],
        json!(["tail", "fifth", "fourth", "third", "wrong"])
    );
    let ability = selection().current["BP11-107"]
        .program
        .card()
        .abilities
        .iter()
        .find(|ability| ability.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, actor())
        .unwrap();
    finish(&mut game, step);
    game.resume_internal_card().unwrap();
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["source"]["acted"], true);
    assert_eq!(after["P1"]["cemetery"], json!(["other"]));
    assert_eq!(after["P1"]["banish"], json!(["a", "b", "c"]));
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(after["P2"]["leader"]["life"], 17_u8);
}

#[test]
fn bp11_108_modes_search_magic_king_card_or_destroy_and_damage_own_leader() {
    for mode in [ModeId(1), ModeId(2)] {
        let mut initial = position("BP11-108");
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"wrong","card":"BP01-037"},{"id":"king-spell","card":"BP11-108"}
        ]);
        let mut game = game(&initial);
        let CardPlay::Awaiting(question) = game.play_internal_card("source", actor()).unwrap()
        else {
            panic!("mode decision")
        };
        assert!(
            matches!(&question.domain, InputDomain::Modes { .. }),
            "the Spell offers its two printed modes"
        );
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Modes(vec![mode]))
            .unwrap();
        finish(&mut game, step);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["hand"],
            if mode == ModeId(1) {
                json!(["king-spell"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P2"]["field"],
            if mode == ModeId(1) {
                json!(["enemy"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            if mode == ModeId(1) { 20_u8 } else { 17_u8 }
        );
        assert_eq!(state["P2"]["leader"]["life"], 20_u8);
    }
}

fn wilderness_position(card: &str, count: usize) -> Value {
    let mut initial = position(card);
    let mut deck = vec![
        json!({"id":"wrong","card":"BP01-037"}),
        json!({"id":"wilderness","card":"BP11-019"}),
    ];
    for n in 2..count {
        deck.push(json!({"id":format!("rest{n}"),"card":"BP01-037"}));
    }
    deck.push(json!({"id":"tail","card":"BP01-037"}));
    initial["players"]["P1"]["zones"]["deck"] = json!(deck);
    initial
}

#[test]
fn bp11_061_takes_wilderness_from_top_three_and_orders_remainder_below_tail() {
    let mut game = game(&wilderness_position("BP11-061", 3));
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["wilderness"]));
    assert_eq!(state["P1"]["deck"], json!(["tail", "rest2", "wrong"]));
}

#[test]
fn bp11_110_real_evolution_takes_wilderness_from_top_four() {
    let mut initial = wilderness_position("BP11-109", 4);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP11-109"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP11-110"}]);
    let mut game = game(&initial);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the printed evolution cost permits the real form"
    );
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["wilderness"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["tail", "rest3", "rest2", "wrong"])
    );
}

#[test]
fn bp11_102_destroys_own_amulet_and_creates_storm_falcon() {
    let mut initial = position("BP11-102");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-016"}]);
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("amulet")),
        "the chosen own amulet pays the sacrifice cost"
    );
    let falcon = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| state["objects"][id.as_str().unwrap()]["name"] == "ホーリーファルコン")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":falcon,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the created Falcon can attack the leader on its entry turn"
    );
}

#[test]
fn bp11_040_recovers_a_golem_and_acts_to_buff_a_field_golem() {
    let mut initial = position("BP11-040");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"wrong","card":"BP01-171"},{"id":"old","card":"BP07-035"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"golem","card":"BP07-035"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["hand"], json!(["old"]));
    assert_eq!(before["P1"]["cemetery"], json!(["wrong"]));
    let id = selection().current["BP11-040"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, actor()).unwrap();
    finish(&mut game, step);
    game.resume_internal_card().unwrap();
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["objects"]["source"]["acted"], true);
    assert_eq!(
        after["objects"]["golem"]["power"].as_u64().unwrap(),
        before["objects"]["golem"]["power"]
            .as_u64()
            .unwrap()
            .checked_add(1)
            .unwrap()
    );
    assert_eq!(
        after["objects"]["golem"]["hp"].as_u64().unwrap(),
        before["objects"]["golem"]["hp"]
            .as_u64()
            .unwrap()
            .checked_add(1)
            .unwrap()
    );
}

#[test]
fn bp11_017_recovers_only_a_puppet_card_and_places_puppet_token_in_ex() {
    let mut initial = position("BP11-017");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"wrong","card":"BP01-171"},{"id":"old","card":"BP11-017"}]);
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["old"]));
    assert_eq!(state["P1"]["cemetery"], json!(["wrong", "source"]));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1);
    let puppet = state["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(state["objects"][puppet]["name"], "操り人形");
}
