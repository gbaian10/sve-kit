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
fn activate(game: &mut Game, source: &str, card: &str) {
    let id = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card(source, id, actor()).unwrap();
    finish(game, step);
    finish_checks(game);
}
fn wound(game: &mut Game, donor: &str, card: &str) {
    let id = selection().current["BP05-059"].program.card().abilities[0]
        .node
        .id;
    let CardPlay::Awaiting(q) = game.activate_internal_card(donor, id, actor()).unwrap() else {
        panic!("damage recipient")
    };
    let InputDomain::Targets(d) = &q.domain else {
        panic!("own follower choice")
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
fn bp08_091_pays_an_own_amulet_before_damage_and_its_controllers_leader() {
    let mut initial = position("BP08-091");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"payment","card":"BP02-076"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    activate(&mut game, "source", "BP08-091");
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["cemetery"], json!(["payment"]));
    assert_eq!(s["objects"]["source"]["acted"], true);
    assert_eq!(s["objects"]["enemy"]["hp"], 1_u8);
    assert_eq!(s["P2"]["leader"]["life"], 19_u8);
}
#[test]
fn bp08_046_discards_two_after_three_draws_then_pays_its_activated_hand_cost() {
    let mut initial = position("BP08-046");
    initial["players"]["P1"]["zones"]["deck"] = Value::Array(
        (0_usize..4)
            .map(|n| json!({"id":format!("draw{n}"),"card":"BP01-171"}))
            .collect(),
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(before["P1"]["cemetery"].as_array().unwrap().len(), 2);
    assert_eq!(before["P1"]["deck"].as_array().unwrap().len(), 1);
    activate(&mut game, "source", "BP08-046");
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["hand"], json!([]));
    assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(after["objects"]["source"]["acted"], true);
}
#[test]
fn bp08_057_heals_after_a_formal_kill_and_returns_itself_to_ex_after_real_lastword_payment() {
    let mut initial = position("BP08-057");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"removal","card":"BP01-116"}),
            json!({"id":"fee1","card":"BP01-171"}),
            json!({"id":"fee2","card":"BP01-171"}),
        ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["leader"]["life"], 25_u8);
    assert_eq!(before["P2"]["cemetery"], json!(["enemy"]));
    play(&mut game, "removal");
    finish_checks(&mut game);
    trigger(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["ex"], json!(["source"]));
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("fee1"))
    );
    assert!(
        after["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("fee2"))
    );
}
#[test]
fn bp08_063_its_own_fanfare_and_a_lethal_own_ability_each_trigger_one_leader_damage() {
    let mut initial = position("BP08-063");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"donor","card":"BP05-059"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        19_u8
    );
    wound(&mut game, "donor", "BP08-063");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P2"]["leader"]["life"], 18_u8);
    assert!(
        s["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}
#[test]
fn bp08_031_a_commander_amulet_counts_as_a_commander_card() {
    for commander in [false, true] {
        let mut initial = position("BP08-031");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"support","card":if commander {"BP01-041"}else{"BP02-076"}}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if commander { 18_u8 } else { 20_u8 }
        );
    }
}
#[test]
fn bp08_079_healing_does_not_clear_crimson_before_its_replacement_draw() {
    for crimson in [false, true] {
        let mut initial = position("BP08-079");
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
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["hand"].as_array().unwrap().len(),
            if crimson { 2 } else { 1 }
        );
        assert_eq!(
            s["P1"]["leader"]["life"],
            if crimson { 21_u8 } else { 22_u8 }
        );
    }
}
#[test]
fn bp08_096_marwyn_removes_the_spell_cost_and_own_aura_does_not_prevent_the_hp_target() {
    for marwyn in [false, true] {
        let mut initial = position("BP08-096");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":if marwyn {"BP05-087"}else{"BP01-171"}}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(s["P1"]["pp"]["current"], if marwyn { 10_u8 } else { 8_u8 });
        assert_eq!(
            s["objects"]["ally"]["hp"],
            if marwyn { 10_u8 } else { 4_u8 }
        );
        assert_eq!(s["P1"]["hand"].as_array().unwrap().len(), 1);
    }
}
#[test]
fn bp08_062_awakened_rush_and_two_damage_events_draw_only_once_in_its_own_turn() {
    let mut initial = position("BP08-062");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"first","card":"BP05-059"},{"id":"second","card":"BP05-059"}]);
    initial["players"]["P1"]["zones"]["deck"] = Value::Array(
        (0_usize..4)
            .map(|n| json!({"id":format!("draw{n}"),"card":"BP01-171"}))
            .collect(),
    );
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
        5_u8
    );
    wound(&mut game, "first", "BP08-062");
    trigger(&mut game);
    wound(&mut game, "second", "BP08-062");
    assert!(game.internal_pending_choice().unwrap().is_none());
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["hand"].as_array().unwrap().len(), 2);
    assert_eq!(s["objects"]["source"]["hp"], 3_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}
#[test]
fn bp08_095_formal_up_to_two_cards_includes_amulets_and_draws_even_with_zero() {
    for take in [0_usize, 2] {
        let mut initial = position("BP08-095");
        initial["players"]["P2"]["zones"]["field"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"amulet","card":"BP02-076"}));
        let mut game = game(&initial);
        play(&mut game, "source");
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("fanfare")
        };
        let CardPlay::Awaiting(q) = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap()
        else {
            panic!("formal cards")
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("up to two")
        };
        assert_eq!(d.cardinality.minimum, Nat(0));
        assert_eq!(d.members.len(), 2);
        let step = game
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Targets(d.members.iter().take(take).map(|m| m.handle).collect()),
            )
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(s["P2"]["field"].as_array().unwrap().len(), 2 - take);
        assert_eq!(s["P1"]["hand"].as_array().unwrap().len(), 1);
    }
}
#[test]
fn bp08_059_damage_increases_maximum_hp_instead_of_erasing_the_damage_mark() {
    let mut initial = position("BP08-059");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"donor","card":"BP05-059"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    wound(&mut game, "donor", "BP08-059");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["source"]["damage"], 1_u8);
    assert_eq!(s["objects"]["source"]["hp"], 4_u8);
    assert_eq!(s["P1"]["leader"]["life"], 21_u8);
}
#[test]
fn bp08_060_real_evolution_keeps_storm_ward_and_its_damage_trigger() {
    let mut initial = field("BP08-059");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"donor","card":"BP05-059"}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP08-060"}]);
    let mut game = game(&initial);
    evolve(&mut game, 3);
    finish_checks(&mut game);
    wound(&mut game, "donor", "BP08-059");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["source"]["hp"], 6_u8);
    assert_eq!(s["P1"]["leader"]["life"], 21_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
    quick_pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
        16_u8
    );
}
#[test]
fn bp08_112_enemy_amulets_count_towards_assail_but_two_cards_are_insufficient() {
    for count in [2_usize, 3] {
        let mut initial = position("BP08-112");
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        for n in 1..count {
            initial["players"]["P2"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("amulet{n}"),"card":"BP02-076"}));
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            if count == 3 { 3_u8 } else { 2_u8 }
        );
        let attack =
            game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}));
        assert_eq!(
            attack.unwrap().outcome,
            if count == 3 {
                "resolved"
            } else {
                "cannot-attack"
            }
        );
    }
}
#[test]
fn bp08_007_storm_attack_counts_itself_and_other_followers_but_not_amulets() {
    for allies in [1_usize, 2] {
        let mut initial = position("BP08-007");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP02-076"}]);
        for n in 0..allies {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("ally{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        trigger(&mut game);
        quick_pass(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P2"]["leader"]["life"],
            if allies == 2 { 16_u8 } else { 17_u8 }
        );
    }
}
#[test]
fn bp08_034_the_buffed_ally_can_rush_a_rested_enemy_but_cannot_attack_a_leader() {
    let mut initial = position("BP08-034");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game, "ally");
    finish_checks(&mut game);
    play(&mut game, "source");
    finish_checks(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["ally"]["power"], 5_u8);
    assert_eq!(s["objects"]["ally"]["hp"], 4_u8);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp08_051_an_idol_card_recovers_pp_independently_of_the_named_rishenna_draw() {
    for idol in [false, true] {
        let mut initial = position("BP08-051");
        initial["players"]["P1"]["pp"]["current"] = json!(4_u8);
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"support","card":if idol {"BP20-050"}else{"BP01-171"}}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(s["P1"]["pp"]["current"], if idol { 5_u8 } else { 4_u8 });
        assert_eq!(s["P1"]["hand"], json!([]));
        assert_eq!(
            s["P1"]["deck"].as_array().unwrap().len(),
            2,
            "without Rishenna no card is drawn"
        );
    }
}
#[test]
fn bp08_043_real_elza_evolution_searches_only_a_magical_creature_follower() {
    let mut initial = evolution("BP08-042", "BP08-043");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"beast","card":"BP08-044"},{"id":"goblin","card":"BP01-171"},{"id":"spell","card":"BP01-007"}]);
    let mut game = game(&initial);
    evolve(&mut game, 2);
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(o) = &q.domain else {
        panic!("evolution trigger")
    };
    let CardPlay::Awaiting(q) = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(o[0].handle))
        .unwrap()
    else {
        panic!("typed search")
    };
    let InputDomain::Search(d) = &q.domain else {
        panic!("private search")
    };
    assert_eq!(d.cards.len(), 1);
    assert_eq!(d.cards[0].card, "BP08-044");
    let step = game
        .answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[0].handle])),
        )
        .unwrap();
    finish(&mut game, step);
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["hand"],
        json!(["beast"])
    );
}
fn unrestricted_search(card: &str) {
    for picked in 0_usize..3 {
        let mut initial = position(card);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"spell","card":"BP01-007"},{"id":"amulet","card":"BP02-076"},{"id":"follower","card":"BP01-171"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(o) = &q.domain else {
            panic!("fanfare")
        };
        let CardPlay::Awaiting(q) = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(o[0].handle))
            .unwrap()
        else {
            panic!("unrestricted search")
        };
        let InputDomain::Search(d) = &q.domain else {
            panic!("private candidates")
        };
        assert_eq!(
            d.cards.len(),
            3,
            "unrestricted search offers every remaining card type"
        );
        assert_eq!(
            d.cardinality.minimum,
            Nat(1),
            "a nonempty unfiltered deck cannot fail to find"
        );
        game.answer_internal_card(
            q.actor,
            q.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![])),
        )
        .unwrap_err();
        let step = game
            .answer_internal_card(
                q.actor,
                q.token,
                Answer::Search(SearchAnswer::Ungrouped(vec![d.cards[picked].handle])),
            )
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        let found = s["P1"]["hand"][0].as_str().unwrap();
        let public = game.projection(View::P2).unwrap();
        assert!(
            public["objects"].get(found).is_none(),
            "an unfiltered search does not reveal its card to the opponent"
        );
        assert!(
            !public["knowledge"]["identifiable"]
                .as_array()
                .unwrap()
                .contains(&json!(found)),
            "the opponent cannot identify the searched card"
        );
        assert_eq!(
            s["P1"]["deck"].as_array().unwrap().len(),
            2,
            "one mandatory card leaves the deck"
        );
    }
}
#[test]
fn bp08_108_unrestricted_search_is_mandatory_and_keeps_the_found_card_private() {
    unrestricted_search("BP08-108");
}
#[test]
fn bp08_u07_unrestricted_search_keeps_the_distinct_printed_characters_card_private() {
    unrestricted_search("BP08-U07");
}
#[test]
fn bp08_069_creates_a_real_rose_strike_that_can_be_played_from_ex() {
    let mut game = game(&position("BP08-069"));
    play(&mut game, "source");
    trigger(&mut game);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(named(&before, "ex", "薔薇の一撃"), 1);
    let token = before["P1"]["ex"][0].as_str().unwrap().to_owned();
    let CardPlay::Awaiting(q) = game.play_internal_card(&token, actor()).unwrap() else {
        panic!("Rose Strike declares an enemy leader or follower")
    };
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("Rose Strike formal recipient")
    };
    let victim = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-037"))
        .unwrap()
        .handle;
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![victim]))
        .unwrap();
    finish(&mut game, step);
    finish_checks(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["ex"], json!([]));
    assert_eq!(after["objects"]["enemy"]["hp"], 2_u8);
    assert_eq!(after["P1"]["hand"].as_array().unwrap().len(), 1);
    assert_eq!(after["P1"]["pp"]["current"], 2_u8);
}
#[test]
fn bp08_100_real_bear_evolution_creates_a_tiger_and_draws_only_with_two_amulets() {
    for count in [1_usize, 2] {
        let mut initial = evolution("BP08-099", "BP08-100");
        for n in 0..count {
            initial["players"]["P1"]["zones"]["field"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("amulet{n}"),"card":"BP02-076"}));
        }
        let mut game = game(&initial);
        evolve(&mut game, 2);
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(named(&s, "field", "ホーリータイガー"), 1);
        assert_eq!(
            s["P1"]["hand"].as_array().unwrap().len(),
            usize::from(count == 2)
        );
        assert_eq!(s["P1"]["pp"]["current"], 8_u8);
    }
}
#[test]
fn bp08_101_the_created_mystic_artifact_runs_its_own_fanfare_draw() {
    let mut game = game(&position("BP08-101"));
    play(&mut game, "source");
    trigger(&mut game);
    while game.internal_pending_choice().unwrap().is_some() {
        trigger_first(&mut game);
    }
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(named(&s, "field", "ミスティックアーティファクト"), 1);
    assert_eq!(s["P1"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(s["P1"]["hand"].as_array().unwrap().len(), 1);
}
#[test]
fn bp08_102_a_real_created_tiger_remains_after_the_amulet_is_paid_for_mieve_and_heals() {
    let mut initial = position("BP08-102");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"payer","card":"BP16-107"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    assert_eq!(
        named(
            &game.projection(View::Referee).unwrap(),
            "field",
            "ホーリータイガー"
        ),
        1
    );
    play(&mut game, "payer");
    trigger(&mut game);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert!(
        s["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert_eq!(
        s["P1"]["leader"]["life"], 25_u8,
        "Mieve heals three and this amulet heals two"
    );
    assert_eq!(named(&s, "field", "ホーリータイガー"), 1);
}
