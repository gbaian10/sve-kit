#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([{"id":format!("{seat}-first"),"card":"BP01-171"},{"id":format!("{seat}-second"),"card":"BP01-171"}]);
    }
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    initial
}

fn field(card: &str) -> Value {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
    initial
}

fn finish_checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn finish(game: &mut Game, mut step: CardPlay) {
    while let CardPlay::Awaiting(q) = step {
        let answer = if let InputDomain::WardEntry { name, .. } = &q.domain {
            Answer::WardEntry(name == "フェアリー")
        } else if matches!(&q.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::Targets(d) = &q.domain {
            Answer::Targets(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::CostMaterials(d) = &q.domain {
            Answer::CostMaterials(
                d.members
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            )
        } else if let InputDomain::Search(d) = &q.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                d.cards
                    .iter()
                    .take(usize::try_from(d.cardinality.maximum.unwrap().0).unwrap())
                    .map(|m| m.handle)
                    .collect(),
            ))
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|m| m.handle).collect())
        } else {
            panic!("unexpected decision")
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
}

fn trigger(game: &mut Game) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending printed ability")
    };
    assert_eq!(
        options.len(),
        1,
        "only the intended printed ability is pending"
    );
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(game, step);
    finish_checks(game);
}

fn evolve(game: &mut Game, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base evolves for its printed PP cost"
    );
}

fn quick_pass(game: &mut Game) {
    let q = game.internal_quick_choice().unwrap().unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Quick(None))
        .unwrap();
    finish(game, step);
}

fn evolution(base: &str, form: &str) -> Value {
    let mut initial = field(base);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn trigger_first(game: &mut Game) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("printed occurrence")
    };
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
        .unwrap();
    finish(game, step);
    finish_checks(game);
}
fn named(s: &Value, zone: &str, name: &str) -> usize {
    s["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| s["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}
fn remove_source(game: &mut Game, card: &str) {
    let CardPlay::Awaiting(q) = game.play_internal_card("removal", actor()).unwrap() else {
        panic!("Soul Conversion target")
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("own follower")
    };
    let h = d
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some(card))
        .unwrap()
        .handle;
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![h]))
        .unwrap();
    finish(game, step);
    finish_checks(game);
}
#[test]
fn bp09_074_formal_follower_and_its_leader_take_four_before_crimson_pp_recovery() {
    for crimson in [false, true] {
        let mut initial = position("BP09-074");
        if crimson {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":"crimson","card":"BP01-118"}));
        }
        let mut game = game(&initial);
        if crimson {
            play(&mut game, "crimson");
            trigger(&mut game);
        }
        let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
            .as_u64()
            .unwrap();
        play(&mut game, "source");
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
        assert_eq!(
            state["P2"]["leader"]["life"],
            if crimson { 15_u8 } else { 16_u8 }
        );
        assert_eq!(
            state["P1"]["pp"]["current"],
            before - 6 + if crimson { 3 } else { 0 }
        );
    }
}
#[test]
fn bp09_084_rushes_then_its_real_lastword_destroys_a_different_follower_and_hurts_that_leader() {
    let mut initial = position("BP09-084");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"removal","card":"BP01-116"}));
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"second","card":"BP01-037"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick_pass(&mut game);
    remove_source(&mut game, "BP09-084");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy", "second"]));
    assert_eq!(state["P2"]["leader"]["life"], 17_u8);
}
#[test]
fn bp09_t01_a_real_ex_spell_token_has_a_required_follower_target_and_controller_damage() {
    let mut initial = field("BP01-171");
    initial["players"]["P1"]["zones"]["field"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP09-T01"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
}
#[test]
fn bp09_052_discards_its_hand_and_both_end_triggers_survive_their_sources_dying() {
    let mut initial = position("BP09-052");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"other","card":"BP09-052"}]);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"discarded","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!([])
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger_first(&mut game);
    trigger_first(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}
#[test]
fn bp09_097_discards_an_amulet_as_a_head_cost_before_five_damage_and_one_draw() {
    let mut initial = position("BP09-097");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fee","card":"BP02-076"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["pp"]["current"], 4_u8);
}
#[test]
fn bp09_098_real_lycaon_evolution_pays_its_own_amulet_discard_before_damage_and_draw() {
    let mut initial = evolution("BP09-097", "BP09-098");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fee","card":"BP02-076"}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["objects"]["source"]["power"], 6_u8);
}
#[test]
fn bp09_053_real_evolution_damages_other_followers_and_the_enemy_leader_at_end() {
    let mut initial = evolution("BP09-052", "BP09-053");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-171"}));
    let mut game = game(&initial);
    evolve(&mut game, 1);
    finish_checks(&mut game);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P2"]["leader"]["life"], 15_u8);
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 1);
}
#[test]
fn bp09_100_any_own_amulet_enables_two_leader_damage_and_rush_is_active_on_entry() {
    for amulet in [false, true] {
        let mut initial = position("BP09-100");
        if amulet {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP02-076"}]);
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if amulet { 18_u8 } else { 20_u8 }
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
                .unwrap()
                .outcome,
            "resolved"
        );
    }
}
#[test]
fn bp09_099_ward_follower_heals_only_when_an_own_amulet_exists() {
    for amulet in [false, true] {
        let mut initial = position("BP09-099");
        if amulet {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP02-076"}]);
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if amulet { 22_u8 } else { 20_u8 }
        );
    }
}
#[test]
fn bp09_110_two_targets_including_an_already_rested_one_skip_exactly_one_opposing_start() {
    let mut initial = position("BP09-110");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"first","card":"BP01-037","state":{"acted":true}},{"id":"second","card":"BP01-037"},{"id":"control","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick_pass(&mut game);
    let first_start = game.projection(View::Referee).unwrap();
    assert_eq!(first_start["objects"]["first"]["acted"], true);
    assert_eq!(first_start["objects"]["second"]["acted"], true);
    assert_eq!(first_start["objects"]["control"]["acted"], false);
    for _ in 0_usize..2 {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        quick_pass(&mut game);
    }
    let later_start = game.projection(View::Referee).unwrap();
    assert_eq!(later_start["objects"]["first"]["acted"], false);
    assert_eq!(later_start["objects"]["second"]["acted"], false);
}
#[test]
fn bp09_091_two_own_amulets_enable_both_fanfare_effects_and_storm_attack() {
    for count in [1_usize, 2] {
        let mut initial = position("BP09-091");
        initial["players"]["P1"]["zones"]["field"] = Value::Array(
            (0..count)
                .map(|n| json!({"id":format!("amulet{n}"),"card":"BP02-076"}))
                .collect(),
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if count == 2 { 5_u8 } else { 3_u8 }
        );
        assert_eq!(
            state["P1"]["leader"]["life"],
            if count == 2 { 22_u8 } else { 20_u8 }
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        quick_pass(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if count == 2 { 15_u8 } else { 17_u8 }
        );
    }
}
#[test]
fn bp09_032_counts_a_token_follower_but_not_an_ordinary_follower() {
    for token in [false, true] {
        let mut initial = position("BP09-032");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"support","card":if token {"BP01-T03"}else{"BP01-171"}}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["objects"]["source"]["power"],
            if token { 3_u8 } else { 2_u8 }
        );
        assert_eq!(
            state["objects"]["source"]["hp"],
            if token { 3_u8 } else { 2_u8 }
        );
    }
}
#[test]
fn bp09_009_real_evolution_buffs_only_the_created_falcon_and_its_bane_wins_a_smaller_battle() {
    let mut initial = position("BP09-008");
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP09-009"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    evolve(&mut game, 1);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let falcon = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| state["objects"][id.as_str().unwrap()]["name"] == "ホーリーファルコン")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(state["objects"][&falcon]["power"], 3_u8);
    assert_eq!(state["objects"]["source"]["power"], 4_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":falcon,"target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick_pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
}
#[test]
fn bp09_010_the_only_legal_beast_target_is_another_own_follower() {
    let mut initial = position("BP09-010");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"beast","card":"BP01-T17"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["beast"]["power"], 5_u8);
    assert_eq!(state["objects"]["beast"]["hp"], 5_u8);
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
}
#[test]
fn bp09_008_creates_an_ordinary_falcon_that_can_storm_the_leader_immediately() {
    let mut game = game(&position("BP09-008"));
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, "field", "ホーリーファルコン"), 1);
    let falcon = state["P1"]["field"]
        .as_array()
        .unwrap()
        .iter()
        .find(|id| state["objects"][id.as_str().unwrap()]["name"] == "ホーリーファルコン")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":falcon,"target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick_pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        18_u8
    );
}

#[test]
fn bp09_023_an_own_prim_reduces_cost_once_and_the_fanfare_still_draws_then_discards_two() {
    for prim in [false, true] {
        let mut initial = position("BP09-023");
        if prim {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"prim","card":"BP09-021"}]);
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], if prim { 8_u8 } else { 7_u8 });
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 2);
        assert_eq!(state["P1"]["deck"], json!([]));
    }
}
#[test]
fn bp09_033_pays_three_hand_cards_before_drawing_three_and_recovering_two_pp() {
    let mut initial = position("BP09-033");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend((0_usize..3).map(|n| json!({"id":format!("fee{n}"),"card":"BP01-171"})));
    initial["players"]["P1"]["zones"]["deck"] = Value::Array(
        (0_usize..4)
            .map(|n| json!({"id":format!("draw{n}"),"card":"BP01-171"}))
            .collect(),
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["fee0", "fee1", "fee2"]));
    assert_eq!(state["P1"]["hand"], json!(["draw0", "draw1", "draw2"]));
    assert_eq!(state["P1"]["deck"], json!(["draw3"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}
#[test]
fn bp09_079_searches_a_beast_follower_then_its_real_amulet_activation_hurts_both_leaders() {
    let mut initial = position("BP09-079");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"BP08-099"},{"id":"spell","card":"BP01-007"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["found"])
    );
    let ability = selection().current["BP09-079"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game
        .activate_internal_card("source", ability, actor())
        .unwrap();
    finish(&mut game, step);
    finish_checks(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["leader"]["life"], 19_u8);
    assert_eq!(state["P2"]["leader"]["life"], 19_u8);
}
#[test]
fn bp09_022_real_prim_evolution_finds_noonya_by_name() {
    let mut initial = evolution("BP09-021", "BP09-022");
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"BP09-023"}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    assert_eq!(state["P1"]["deck"], json!(["wrong"]));
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
}
#[test]
fn bp09_027_creates_a_knight_on_fanfare_and_a_separate_ex_knight_when_rushing() {
    let mut game = game(&position("BP09-027"));
    play(&mut game, "source");
    trigger(&mut game);
    assert_eq!(
        named(&game.projection(View::Referee).unwrap(), "field", "ナイト"),
        1
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game);
    assert_eq!(
        named(&game.projection(View::Referee).unwrap(), "ex", "ナイト"),
        1
    );
    quick_pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, "field", "ナイト"), 1);
    assert_eq!(state["objects"]["enemy"]["hp"], 1_u8);
}
#[test]
fn bp09_062_awakened_creates_two_orcas_with_the_printed_field_capacity_limit() {
    for (maximum, fill) in [(6_u8, 0_usize), (7, 0), (7, 4)] {
        let mut initial = position("BP09-062");
        initial["players"]["P1"]["pp"] = json!({"current":maximum,"max":maximum});
        initial["players"]["P1"]["zones"]["field"] = Value::Array(
            (0..fill)
                .map(|n| json!({"id":format!("ally{n}"),"card":"BP01-171"}))
                .collect(),
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        assert_eq!(
            named(&game.projection(View::Referee).unwrap(), "field", "オルカ"),
            (if maximum == 7 { 2_usize } else { 1 }).min(5 - fill)
        );
    }
}
#[test]
fn bp09_014_only_the_two_new_fairies_gain_stats_and_ward() {
    let mut initial = position("BP09-014");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"old","card":"BP01-T03"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, "field", "フェアリー"), 3);
    assert_eq!(state["objects"]["old"]["power"], 1_u8);
    assert_eq!(state["objects"]["old"]["hp"], 1_u8);
    for id in state["P1"]["field"].as_array().unwrap() {
        let id = id.as_str().unwrap();
        if id != "old" && id != "source" {
            assert_eq!(state["objects"][id]["power"], 2_u8);
            assert_eq!(state["objects"][id]["hp"], 2_u8);
            assert_eq!(state["objects"][id]["keywords"], json!(["守護"]));
            assert_eq!(
                state["objects"][id]["acted"], false,
                "Fairies enter before this ability grants them Ward"
            );
        }
    }
}
