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
                    .rev()
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
#[test]
fn bp07_028_without_mistrina_deals_two_only_to_its_formal_follower() {
    let mut game = game(&position("BP07-028"));
    play(&mut game, "source");
    finish_checks(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["enemy"]["hp"], 3_u8);
    assert_eq!(s["P2"]["leader"]["life"], 20_u8);
}
#[test]
fn bp07_079_only_nicola_replaces_four_damage_with_six() {
    for nicola in [false, true] {
        let mut initial = position("BP07-079");
        if nicola {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"nicola","card":"BP07-076"}]);
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        if nicola {
            assert_eq!(s["P2"]["cemetery"], json!(["enemy"]));
        } else {
            assert_eq!(s["objects"]["enemy"]["hp"], 1_u8);
        }
        assert_eq!(s["P2"]["leader"]["life"], 20_u8);
    }
}
#[test]
fn bp07_074_real_evolution_uses_the_necrocharge_nine_ten_boundary() {
    for count in [9_usize, 10] {
        let mut initial = evolution("BP07-073", "BP07-074");
        initial["players"]["P1"]["zones"]["cemetery"] = Value::Array(
            (0..count)
                .map(|n| json!({"id":format!("grave{n}"),"card":"BP01-171"}))
                .collect(),
        );
        let mut game = game(&initial);
        evolve(&mut game, 1);
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["enemy"]["hp"],
            if count == 10 { 1_u8 } else { 3_u8 }
        );
        assert_eq!(s["P1"]["cemetery"].as_array().unwrap().len(), count);
    }
}
#[test]
fn bp07_067_sacrifices_itself_before_damaging_all_remaining_followers_and_its_own_leader() {
    let mut initial = position("BP07-067");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    activate(&mut game, "source", "BP07-067");
    let s = game.projection(View::Referee).unwrap();
    assert!(
        s["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
    assert!(
        s["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("ally"))
    );
    assert_eq!(s["objects"]["enemy"]["hp"], 2_u8);
    assert_eq!(s["P1"]["leader"]["life"], 17_u8);
    assert_eq!(s["P2"]["leader"]["life"], 20_u8);
    assert_eq!(s["P1"]["pp"]["current"], 6_u8);
}
#[test]
fn bp07_100_storm_attacks_and_independently_selects_a_leader_or_follower() {
    for leader in [false, true] {
        let mut game = game(&position("BP07-100"));
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
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("attack occurrence")
        };
        let CardPlay::Awaiting(q) = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap()
        else {
            panic!("attack target")
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("recipient")
        };
        let h = d
            .members
            .iter()
            .find(|m| m.card.is_none() == leader)
            .unwrap()
            .handle;
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![h]))
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        quick_pass(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P2"]["leader"]["life"],
            if leader { 18_u8 } else { 19_u8 }
        );
        assert_eq!(
            s["objects"]["enemy"]["hp"],
            if leader { 5_u8 } else { 4_u8 }
        );
    }
}
#[test]
fn bp07_059_fanfare_counts_itself_and_only_its_own_ocean_followers() {
    for other in [1_usize, 2] {
        let mut initial = position("BP07-059");
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut allies: Vec<Value> = (0..other)
            .map(|n| json!({"id":format!("orca{n}"),"card":"BP02-T05"}))
            .collect();
        allies.push(json!({"id":"wrong","card":"BP01-171"}));
        initial["players"]["P1"]["zones"]["field"] = Value::Array(allies);
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            11 + u64::try_from(other).unwrap()
        );
    }
}
#[test]
fn bp07_029_rests_its_own_tree_to_gain_entry_turn_storm() {
    let mut initial = position("BP07-029");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"tree","card":"BP07-T03"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["tree"]["acted"],
        true
    );
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
fn bp07_083_real_evolution_destroys_two_followers_and_excludes_amulets() {
    let mut initial = evolution("BP07-082", "BP07-083");
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"},{"id":"second","card":"BP01-037"},{"id":"amulet","card":"BP19-108"}]);
    let mut game = game(&initial);
    evolve(&mut game, 3);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P2"]["cemetery"], json!(["enemy", "second"]));
    assert_eq!(s["P2"]["field"], json!(["amulet"]));
    assert_eq!(s["P1"]["pp"]["current"], 7_u8);
}
#[test]
fn bp07_039_discards_a_spell_as_a_head_cost_before_drawing_one() {
    let mut initial = position("BP07-039");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"fee","card":"BP01-007"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(s["P1"]["hand"], json!(["P1-first"]));
    assert_eq!(s["P1"]["pp"]["current"], 8_u8);
}
#[test]
fn bp07_076_fanfare_mills_two_of_its_own_top_cards_without_drawing() {
    let mut initial = position("BP07-076");
    initial["players"]["P1"]["zones"]["deck"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"tail","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["cemetery"], json!(["P1-first", "P1-second"]));
    assert_eq!(s["P1"]["deck"], json!(["tail"]));
    assert_eq!(s["P1"]["hand"], json!([]));
    assert_eq!(s["P2"]["deck"].as_array().unwrap().len(), 2);
}
#[test]
fn bp07_017_combo_creates_two_trees_and_heals_only_after_three_real_plays() {
    for prior in [0_usize, 2] {
        let mut initial = position("BP07-017");
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
            named(&s, "ex", "ナテラの大樹"),
            if prior == 2 { 2 } else { 1 }
        );
        assert_eq!(
            s["P1"]["leader"]["life"],
            if prior == 2 { 22_u8 } else { 20_u8 }
        );
    }
}
#[test]
fn bp07_014_fanfare_creates_one_tree_in_ex_without_putting_it_on_field() {
    let mut game = game(&position("BP07-014"));
    play(&mut game, "source");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(named(&s, "ex", "ナテラの大樹"), 1);
    assert_eq!(s["P1"]["field"], json!(["source"]));
    assert_eq!(s["P1"]["pp"]["current"], 9_u8);
}
#[test]
fn bp07_013_a_real_tree_pays_the_once_per_turn_heal_and_still_triggers_when_banished() {
    let mut initial = position("BP07-013");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"spare","card":"BP07-T03"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let created = game.projection(View::Referee).unwrap();
    let tree = created["P1"]["ex"][0].as_str().unwrap().to_owned();
    play(&mut game, &tree);
    finish_checks(&mut game);
    activate(&mut game, "source", "BP07-013");
    while game.internal_pending_choice().unwrap().is_some() {
        trigger_first(&mut game);
    }
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["leader"]["life"], 23_u8);
    assert_eq!(after["P1"]["banish"], json!([]));
    assert_eq!(after["P1"]["deck"].as_array().unwrap().len(), 1);
    assert_eq!(after["P1"]["hand"], json!([]));
    let id = selection().current["BP07-013"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    game.activate_internal_card("source", id, actor())
        .unwrap_err();
}
#[test]
fn bp07_033_creates_one_tree_on_field_and_another_in_ex() {
    let mut game = game(&position("BP07-033"));
    play(&mut game, "source");
    finish_checks(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(named(&s, "field", "ナテラの大樹"), 1);
    assert_eq!(named(&s, "ex", "ナテラの大樹"), 1);
}
#[test]
fn bp07_060_banishes_a_tree_before_buffing_and_its_real_lastword_replaces_the_tree() {
    let mut initial = position("BP07-060");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"tree","card":"BP07-T03"}]);
    // The tree's draw/discard must leave Soul Conversion for the real Lastword path.
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"discard","card":"BP01-171"}),
            json!({"id":"removal","card":"BP01-116"}),
        ]);
    initial["players"]["P1"]["zones"]["deck"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"spare0","card":"BP01-171"}),
            json!({"id":"spare1","card":"BP01-171"}),
        ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    let buffed = game.projection(View::Referee).unwrap();
    assert_eq!(buffed["objects"]["source"]["power"], 5_u8);
    assert_eq!(buffed["objects"]["source"]["hp"], 5_u8);
    assert_eq!(buffed["P1"]["banish"], json!([]));
    while game.internal_pending_choice().unwrap().is_some() {
        trigger_first(&mut game);
    }
    let before_removal = game.projection(View::Referee).unwrap();
    assert!(
        before_removal["P1"]["hand"]
            .as_array()
            .unwrap()
            .contains(&json!("removal")),
        "the tree must leave a real removal card available"
    );
    remove_source(&mut game, "BP07-060");
    trigger(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(named(&after, "field", "ナテラの大樹"), 1);
}
#[test]
fn bp07_061_real_evolution_deals_three_and_lastword_creates_a_tree() {
    let mut initial = evolution("BP07-060", "BP07-061");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"removal","card":"BP01-116"}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["hp"],
        2_u8
    );
    remove_source(&mut game, "BP07-060");
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(named(&s, "field", "ナテラの大樹"), 1);
    assert!(
        s["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source"))
    );
}
#[test]
fn bp07_062_banishes_a_tree_for_pp_max_and_restores_one_with_its_lastword() {
    let mut initial = position("BP07-062");
    initial["players"]["P1"]["pp"] = json!({"current":6_u8,"max":6_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"tree","card":"BP07-T03"}]);
    // The tree's draw/discard must leave Soul Conversion for the real Lastword path.
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"discard","card":"BP01-171"}),
            json!({"id":"removal","card":"BP01-116"}),
        ]);
    initial["players"]["P1"]["zones"]["deck"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"spare0","card":"BP01-171"}),
            json!({"id":"spare1","card":"BP01-171"}),
        ]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["max"],
        7_u8
    );
    while game.internal_pending_choice().unwrap().is_some() {
        trigger_first(&mut game);
    }
    remove_source(&mut game, "BP07-062");
    trigger(&mut game);
    assert_eq!(
        named(
            &game.projection(View::Referee).unwrap(),
            "field",
            "ナテラの大樹"
        ),
        1
    );
}
