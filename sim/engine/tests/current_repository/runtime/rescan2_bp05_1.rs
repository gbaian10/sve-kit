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
        let answer = if matches!(&q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
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

#[test]
fn bp05_117_normal_damage_keeps_the_target_and_its_leader_alive() {
    let mut game = game(&position("BP05-117"));
    play(&mut game, "source");
    finish_checks(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["enemy"]["hp"], 3_u8);
    assert_eq!(s["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp05_050_only_lio_replaces_three_damage_and_adds_leader_damage() {
    for lio in [false, true] {
        let mut initial = position("BP05-050");
        if lio {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"lio","card":"BP05-035"}]);
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(s["P2"]["leader"]["life"], if lio { 17_u8 } else { 20_u8 });
        if lio {
            assert_eq!(s["P2"]["cemetery"], json!(["enemy"]));
        } else {
            assert_eq!(s["objects"]["enemy"]["hp"], 2_u8);
        }
    }
}

#[test]
fn bp05_073_end_ability_checks_the_opponents_hand_at_resolution() {
    for hand in [3_usize, 4] {
        let mut initial = field("BP05-073");
        initial["players"]["P2"]["zones"]["hand"] = Value::Array(
            (0..hand)
                .map(|n| json!({"id":format!("held{n}"),"card":"BP01-171"}))
                .collect(),
        );
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if hand == 3 { 17_u8 } else { 20_u8 }
        );
    }
}

#[test]
fn bp05_074_real_evolution_destroys_instead_of_dealing_three_at_three_cards() {
    for hand in [3_usize, 4] {
        let mut initial = evolution("BP05-073", "BP05-074");
        initial["players"]["P2"]["zones"]["hand"] = Value::Array(
            (0..hand)
                .map(|n| json!({"id":format!("held{n}"),"card":"BP01-171"}))
                .collect(),
        );
        let mut game = game(&initial);
        evolve(&mut game, 1);
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        if hand == 3 {
            assert_eq!(s["P2"]["cemetery"], json!(["enemy"]));
        } else {
            assert_eq!(s["objects"]["enemy"]["hp"], 2_u8);
        }
        assert_eq!(s["P1"]["pp"]["current"], 9_u8);
    }
}

#[test]
fn bp05_045_both_leader_clauses_require_two_idol_cards() {
    for idols in [1_usize, 2] {
        let mut initial = position("BP05-045");
        initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
        initial["players"]["P1"]["zones"]["field"] = Value::Array(
            (0..idols)
                .map(|n| json!({"id":format!("idol{n}"),"card":"BP05-047"}))
                .collect(),
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(s["objects"]["enemy"]["hp"], 3_u8);
        assert_eq!(
            s["P2"]["leader"]["life"],
            if idols == 2 { 18_u8 } else { 20_u8 }
        );
        assert_eq!(
            s["P1"]["leader"]["life"],
            if idols == 2 { 17_u8 } else { 15_u8 }
        );
    }
}

#[test]
fn bp05_023_end_trigger_damages_its_followers_controller() {
    let mut game = game(&field("BP05-023"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["enemy"]["hp"], 1_u8);
    assert_eq!(s["P2"]["leader"]["life"], 18_u8);
    assert_eq!(s["P1"]["leader"]["life"], 20_u8);
}

#[test]
fn bp05_035_fanfare_pays_a_mage_card_before_nine_damage() {
    let mut initial = position("BP05-035");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fee","card":"BP05-050"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(s["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(s["P1"]["pp"]["current"], 3_u8);
}

#[test]
fn bp05_058_requires_both_formal_targets_and_damages_them_differently() {
    let mut initial = position("BP05-058");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-174"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["ally"]["hp"], 3_u8);
    assert_eq!(s["objects"]["enemy"]["hp"], 2_u8);
    let mut missing = initial;
    missing["players"]["P1"]["zones"]["field"] = json!([]);
    let mut rejected = super::game(&missing);
    let before = rejected.projection(View::Referee).unwrap();
    rejected.play_internal_card("source", actor()).unwrap_err();
    assert_eq!(rejected.projection(View::Referee).unwrap(), before);
}

#[test]
fn bp05_038_fanfare_counts_itself_but_not_an_unrelated_follower() {
    for others in [1_usize, 2] {
        let mut initial = position("BP05-038");
        let mut allies: Vec<Value> = (0..others)
            .map(|n| json!({"id":format!("mage{n}"),"card":"BP05-038"}))
            .collect();
        allies.push(json!({"id":"goblin","card":"BP01-171"}));
        initial["players"]["P1"]["zones"]["field"] = Value::Array(allies);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["source"]["power"],
            if others == 2 { 5_u8 } else { 3_u8 }
        );
        assert_eq!(
            s["objects"]["source"]["hp"],
            if others == 2 { 5_u8 } else { 3_u8 }
        );
    }
}

#[test]
fn bp05_039_real_evolution_deals_twice_the_current_mage_follower_count() {
    let mut initial = evolution("BP05-038", "BP05-039");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"mage","card":"BP05-038"}));
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["enemy"]["hp"], 1_u8);
    assert_eq!(s["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp05_047_fanfare_includes_its_own_idol_trait() {
    for others in [1_usize, 2] {
        let mut initial = position("BP05-047");
        initial["players"]["P1"]["zones"]["field"] = Value::Array(
            (0..others)
                .map(|n| json!({"id":format!("idol{n}"),"card":"BP05-047"}))
                .collect(),
        );
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["hand"].as_array().unwrap().len(),
            if others == 2 { 2 } else { 0 }
        );
    }
}

#[test]
fn bp05_055_ability_damage_grants_storm_and_only_one_power() {
    let mut initial = position("BP05-055");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"claw","card":"BP05-058"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    play(&mut game, "claw");
    finish_checks(&mut game);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["source"]["power"], 3_u8);
    assert_eq!(s["objects"]["source"]["hp"], 3_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick_pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        17_u8
    );
}

#[test]
fn bp05_062_optional_fanfare_discards_then_rush_attacks_twice_but_stands_once() {
    let mut initial = position("BP05-062");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fee","card":"BP01-171"}));
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-171","state":{"acted":true}},{"id":"enemy2","card":"BP01-171","state":{"acted":true}}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    for target in ["enemy", "enemy2"] {
        assert_eq!(
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":target}))
                .unwrap()
                .outcome,
            "resolved"
        );
        if target == "enemy" {
            trigger(&mut game);
        } else {
            assert!(
                game.internal_pending_choice().unwrap().is_none(),
                "only the first attack triggers a stand"
            );
        }
        quick_pass(&mut game);
    }
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(s["P2"]["cemetery"], json!(["enemy", "enemy2"]));
    assert_eq!(s["objects"]["source"]["acted"], true);
    assert_eq!(s["objects"]["source"]["hp"], 3_u8);
}

#[test]
fn bp05_104_real_evolution_end_stands_and_applies_both_empty_hand_clauses() {
    for hand in [0_usize, 2, 3] {
        let mut initial = evolution("BP05-103", "BP05-104");
        initial["players"]["P1"]["zones"]["hand"] = Value::Array(
            (0..hand)
                .map(|n| json!({"id":format!("held{n}"),"card":"BP01-171"}))
                .collect(),
        );
        let mut game = game(&initial);
        evolve(&mut game, 1);
        finish_checks(&mut game);
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
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(s["objects"]["source"]["acted"], false);
        assert_eq!(
            s["P2"]["leader"]["life"],
            if hand <= 2 { 9_u8 } else { 14_u8 }
        );
        if hand == 0 {
            assert_eq!(s["P2"]["cemetery"], json!(["enemy"]));
        } else {
            assert_eq!(s["objects"]["enemy"]["hp"], 5_u8);
        }
    }
}

#[test]
fn bp05_008_only_a_hunter_top_card_can_move_to_hand_without_bottoming_the_rest() {
    for top in ["BP05-004", "BP01-171"] {
        let mut initial = position("BP05-008");
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"top","card":top},{"id":"tail","card":"BP01-171"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["hand"],
            if top == "BP05-004" {
                json!(["top"])
            } else {
                json!([])
            }
        );
        assert_eq!(
            s["P1"]["deck"],
            if top == "BP05-004" {
                json!(["tail"])
            } else {
                json!(["top", "tail"])
            }
        );
    }
}

#[test]
fn bp05_026_discards_a_commander_card_then_searches_past_an_ineligible_card() {
    let mut initial = position("BP05-026");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fee","card":"BP05-023"}));
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"BP05-023"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["hand"], json!(["found"]));
    assert_eq!(s["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(s["P1"]["deck"], json!(["wrong", "tail"]));
}

#[test]
fn bp05_088_end_mode_can_discard_even_zero_cards_or_recover_four_pp() {
    use sve_engine::ir::ModeId;
    for (mode, hand) in [(1_u64, 0_usize), (1, 3), (2, 3)] {
        let mut initial = field("BP05-088");
        initial["players"]["P1"]["pp"]["current"] = json!(3_u8);
        initial["players"]["P1"]["zones"]["hand"] = Value::Array(
            (0..hand)
                .map(|n| json!({"id":format!("held{n}"),"card":"BP01-171"}))
                .collect(),
        );
        initial["players"]["P1"]["zones"]["deck"] = Value::Array(
            (0_usize..4)
                .map(|n| json!({"id":format!("drawn{n}"),"card":"BP01-171"}))
                .collect(),
        );
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved"
        );
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("End pending occurrence")
        };
        let CardPlay::Awaiting(q) = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap()
        else {
            panic!("printed End mode")
        };
        assert!(matches!(&q.domain, InputDomain::Modes { .. }));
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Modes(vec![ModeId(mode)]))
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["hand"].as_array().unwrap().len(),
            if mode == 1 { 4 } else { hand }
        );
        assert_eq!(
            s["P1"]["cemetery"].as_array().unwrap().len(),
            if mode == 1 { hand } else { 0 }
        );
        assert_eq!(
            s["P1"]["pp"]["current"],
            if mode == 1 { 3_u8 } else { 7_u8 }
        );
    }
}

#[test]
fn bp05_096_at_five_cards_recovers_pp_and_draws_without_rechecking_the_first_clause() {
    for retained in [4_usize, 5, 6] {
        let mut initial = position("BP05-096");
        for n in 0..retained {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("held{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["pp"]["current"],
            if retained >= 5 { 8_u8 } else { 6_u8 }
        );
        assert_eq!(
            s["P1"]["hand"].as_array().unwrap().len(),
            retained + usize::from(retained <= 5)
        );
    }
}
