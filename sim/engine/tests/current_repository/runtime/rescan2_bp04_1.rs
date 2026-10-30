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

fn pay_life(game: &mut Game) {
    play(game, "payer");
    trigger(game);
}

#[test]
fn bp04_080_fanfare_replaces_four_damage_with_eight_after_real_life_loss() {
    for crimson in [false, true] {
        let mut initial = position("BP04-080");
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"payer","card":"BP01-118"}));
        let mut game = game(&initial);
        if crimson {
            pay_life(&mut game);
        }
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        if crimson {
            assert_eq!(
                s["P2"]["cemetery"],
                json!(["enemy"]),
                "eight damage kills the five-health target"
            );
        } else {
            assert_eq!(
                s["objects"]["enemy"]["damage"], 4_u8,
                "without Crimson the Fanfare deals exactly four"
            );
        }
        assert_eq!(
            s["P2"]["leader"]["life"],
            if crimson { 19_u8 } else { 20_u8 },
            "the life-loss setup damages both leaders once; this Fanfare adds no leader damage"
        );
    }
}

#[test]
fn bp04_081_real_evolution_heals_only_in_crimson_and_has_storm() {
    for crimson in [false, true] {
        let mut initial = evolution("BP04-080", "BP04-081");
        initial["players"]["P1"]["zones"]["field"][0]["state"]["entered_this_turn"] = json!(true);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"payer","card":"BP01-118"}]);
        let mut game = game(&initial);
        if crimson {
            pay_life(&mut game);
        }
        evolve(&mut game, 3);
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if crimson { 24_u8 } else { 20_u8 },
            "the five-life evolution heal requires actual life loss this turn"
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved",
            "printed Storm permits the evolved source to attack the leader on entry"
        );
    }
}

fn commander_damage(card: &str) {
    let mut initial = position(card);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"commander","card":"BP04-030"},{"id":"decoy","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"},{"id":"second","card":"BP01-037"},{"id":"amulet","card":"BP19-108"}]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(q) = game.play_internal_card("source", actor()).unwrap() else {
        panic!("formal Commander target")
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("Commander target domain")
    };
    assert_eq!(
        d.members.len(),
        1,
        "only the allied Commander is a legal power source"
    );
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
        .unwrap();
    finish(&mut game, step);
    finish_checks(&mut game);
    let s = game.projection(View::Referee).unwrap();
    for id in ["enemy", "second"] {
        assert_eq!(
            s["objects"][id]["damage"], 3_u8,
            "the selected Commander's power damages every opposing follower"
        );
    }
    assert_eq!(
        s["objects"]["commander"]["damage"], 0_u8,
        "the allied Commander receives no sweep damage"
    );
    assert_eq!(
        s["P1"]["field"],
        json!(["commander", "decoy"]),
        "allied followers remain on the field"
    );
    assert_eq!(
        s["P2"]["field"],
        json!(["enemy", "second", "amulet"]),
        "the opposing amulet is not a follower recipient"
    );
}

#[test]
fn bp04_026_selected_commander_supplies_the_enemy_follower_sweep() {
    commander_damage("BP04-026");
}
#[test]
fn bp04_027_selected_commander_supplies_the_enemy_follower_sweep() {
    commander_damage("BP04-027");
}

#[test]
fn bp04_019_combo_follows_a_lethally_damaged_targets_leader() {
    for prior in [0_usize, 2] {
        let mut initial = position("BP04-019");
        for n in 0..prior {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("prior{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        for n in 0..prior {
            play(&mut game, &format!("prior{n}"));
            finish_checks(&mut game);
        }
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P2"]["cemetery"],
            json!(["enemy"]),
            "five damage kills the selected follower in either case"
        );
        assert_eq!(
            s["P2"]["leader"]["life"],
            if prior == 2 { 17_u8 } else { 20_u8 },
            "Combo three adds leader damage using the selected follower's controller"
        );
    }
}

#[test]
fn bp04_033_attack_damages_its_selected_follower_and_that_leader() {
    let mut game = game(&field("BP04-033"));
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the standing follower declares a leader attack"
    );
    trigger(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(
        before["objects"]["enemy"]["damage"], 1_u8,
        "the attack-time ability damages the selected follower"
    );
    assert_eq!(
        before["P2"]["leader"]["life"], 19_u8,
        "the attack-time ability also damages that follower's leader"
    );
    quick_pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        17_u8,
        "ordinary attack damage follows the triggered damage"
    );
}

#[test]
fn bp04_099_damages_other_followers_then_buffs_only_surviving_allies() {
    let mut initial = position("BP04-099");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":"BP01-037"},{"id":"amulet","card":"BP19-108"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["objects"]["source"]["damage"], 0_u8,
        "other excludes the Fanfare source from damage"
    );
    assert_eq!(
        s["objects"]["source"]["power"], 5_u8,
        "other excludes the source from the power bonus"
    );
    assert_eq!(
        s["objects"]["ally"]["damage"], 2_u8,
        "the allied follower takes the symmetric sweep"
    );
    assert_eq!(
        s["objects"]["ally"]["power"], 7_u8,
        "the surviving allied follower gains two power"
    );
    assert_eq!(
        s["objects"]["enemy"]["damage"], 2_u8,
        "the opposing follower takes two damage"
    );
    assert_eq!(
        s["objects"]["enemy"]["power"], 5_u8,
        "the opponent receives no power bonus"
    );
}

#[test]
fn bp04_012_fanfare_and_lastword_share_the_printed_line_but_fire_separately() {
    let mut initial = position("BP04-012");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"removal","card":"BP01-116"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let CardPlay::Awaiting(q) = game.play_internal_card("removal", actor()).unwrap() else {
        panic!("Soul Conversion target")
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("own follower target")
    };
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![d.members[0].handle]))
        .unwrap();
    finish(&mut game, step);
    finish_checks(&mut game);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["objects"]["enemy"]["damage"], 2_u8,
        "Fanfare and Lastword each deal one to opposing followers"
    );
    assert_eq!(
        s["P2"]["leader"]["life"], 18_u8,
        "the two printed triggers each damage the opposing leader"
    );
    assert_eq!(
        s["P1"]["leader"]["life"], 20_u8,
        "neither trigger damages its controller"
    );
}

#[test]
fn bp04_120_evolves_normally_then_pays_four_for_its_enemy_only_sweep() {
    let mut game = game(&evolution("BP04-119", "BP04-120"));
    evolve(&mut game, 2);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(
        s["P1"]["pp"]["current"], 4_u8,
        "normal evolution and the paid trigger cost two plus four PP"
    );
    assert_eq!(
        s["P2"]["leader"]["life"], 16_u8,
        "the paid trigger damages opposing leaders by four"
    );
    assert_eq!(
        s["objects"]["enemy"]["damage"], 4_u8,
        "the paid trigger also damages opposing followers"
    );
    assert_eq!(
        s["P1"]["leader"]["life"], 20_u8,
        "the controller leader is excluded"
    );
}

#[test]
fn bp04_030_juliet_discount_is_not_multiplied_and_end_heal_requires_juliet() {
    for count in [0_usize, 1, 2] {
        let mut initial = position("BP04-030");
        initial["players"]["P1"]["zones"]["field"] = json!([]);
        for n in 0..count {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("juliet{n}"),"card":"BP04-032"}));
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
            if count == 0 { 7_u8 } else { 9_u8 },
            "one or more Juliets reduce the play cost by exactly two"
        );
        assert_eq!(
            game.internal_rule_action(&json!({"do":"end-phase"}))
                .unwrap()
                .outcome,
            "resolved",
            "the controller enters its real End phase"
        );
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("pending End abilities")
        };
        let option = options.iter().find(|o| o.card == "BP04-030").unwrap();
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if count == 0 { 20_u8 } else { 22_u8 },
            "End healing requires Juliet and does not multiply with her count"
        );
    }
}

#[test]
fn bp04_051_fanfare_requires_an_actual_spell_in_own_cemetery() {
    for buried in ["BP01-171", "BP01-007"] {
        let mut initial = position("BP04-051");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"buried","card":buried}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if buried == "BP01-007" { 22_u8 } else { 20_u8 },
            "only an actual spell in the controller cemetery enables the heal"
        );
    }
}
