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
fn bp06_067_declares_an_awakened_distribution_before_resolving_damage() {
    use sve_engine::game::internal::DamageShare;
    for max in [6_u8, 7] {
        let mut initial = position("BP06-067");
        initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP01-037"},{"id":"second","card":"BP01-037"}]);
        let mut game = game(&initial);
        play(&mut game, "source");
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("Fanfare occurrence")
        };
        let CardPlay::Awaiting(q) = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap()
        else {
            panic!("formal targets")
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("two follower targets")
        };
        let handles = d.members.iter().map(|m| m.handle).collect();
        let CardPlay::Awaiting(q) = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(handles))
            .unwrap()
        else {
            panic!("damage allocation")
        };
        let InputDomain::Distribution(distribution) = &q.domain else {
            panic!("frozen recipients")
        };
        assert_eq!(distribution.total, Nat(if max == 7 { 4 } else { 2 }));
        let shares = distribution
            .members
            .iter()
            .map(|m| DamageShare {
                handle: m.handle,
                amount: Nat(if max == 7 { 2 } else { 1 }),
            })
            .collect();
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Distribution(shares))
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        for id in ["enemy", "second"] {
            assert_eq!(s["objects"][id]["hp"], if max == 7 { 3_u8 } else { 4_u8 });
        }
        assert_eq!(s["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn bp06_013_hunter_discard_pays_for_either_a_leader_or_follower_and_one_draw() {
    for leader in [false, true] {
        let mut initial = position("BP06-013");
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"fee","card":"BP05-004"}));
        let mut game = game(&initial);
        play(&mut game, "source");
        let q = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &q.domain else {
            panic!("Fanfare occurrence")
        };
        let CardPlay::Awaiting(q) = game
            .answer_internal_card(q.actor, q.token, Answer::Pending(options[0].handle))
            .unwrap()
        else {
            panic!("formal recipient")
        };
        assert_eq!(q.domain, InputDomain::OptionalCost);
        let CardPlay::Awaiting(q) = game
            .answer_internal_card(q.actor, q.token, Answer::OptionalCost(true))
            .unwrap()
        else {
            panic!("formal recipient after declaration")
        };
        let InputDomain::Targets(d) = &q.domain else {
            panic!("leader or follower")
        };
        let selected = d
            .members
            .iter()
            .find(|m| m.card.is_none() == leader)
            .unwrap()
            .handle;
        let step = game
            .answer_internal_card(q.actor, q.token, Answer::Targets(vec![selected]))
            .unwrap();
        finish(&mut game, step);
        finish_checks(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(s["P1"]["cemetery"], json!(["fee"]));
        assert_eq!(s["P1"]["hand"], json!(["P1-first"]));
        assert_eq!(
            s["P2"]["leader"]["life"],
            if leader { 19_u8 } else { 20_u8 }
        );
        assert_eq!(
            s["objects"]["enemy"]["hp"],
            if leader { 5_u8 } else { 4_u8 }
        );
    }
}

#[test]
fn bp06_026_real_evolution_discards_a_card_before_five_damage() {
    let mut initial = evolution("BP06-025", "BP06-026");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fee","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(s["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(s["objects"]["source"]["power"], 4_u8);
    assert_eq!(s["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp06_116_uses_both_selected_powers_even_when_the_first_victim_dies() {
    let mut initial = position("BP06-116");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    finish_checks(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert!(
        s["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("ally"))
    );
    assert_eq!(s["objects"]["enemy"]["hp"], 3_u8);
    assert_eq!(s["P2"]["leader"]["life"], 20_u8);
}

#[test]
fn bp06_071_amulet_presence_replaces_two_healing_with_four() {
    for amulet in [false, true] {
        let mut initial = position("BP06-071");
        initial["players"]["P1"]["leader"]["life"] = json!(15_u8);
        if amulet {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP19-108"}]);
        }
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["P1"]["leader"]["life"],
            if amulet { 19_u8 } else { 17_u8 }
        );
        assert_eq!(s["P2"]["leader"]["life"], 20_u8);
    }
}

#[test]
fn bp06_063_only_awakened_fanfare_adds_power_and_entry_turn_storm() {
    for max in [6_u8, 7] {
        let mut initial = position("BP06-063");
        initial["players"]["P1"]["pp"] = json!({"current":max,"max":max});
        let mut game = game(&initial);
        play(&mut game, "source");
        trigger(&mut game);
        let s = game.projection(View::Referee).unwrap();
        assert_eq!(
            s["objects"]["source"]["power"],
            if max == 7 { 3_u8 } else { 2_u8 }
        );
        assert_eq!(s["objects"]["source"]["hp"], 2_u8);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            if max == 7 {
                "resolved"
            } else {
                "cannot-attack"
            }
        );
    }
}

#[test]
fn bp06_027_stands_only_after_the_first_attack_of_the_turn() {
    let mut game = game(&field("BP06-027"));
    for first in [true, false] {
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved"
        );
        if first {
            trigger(&mut game);
        } else {
            assert!(game.internal_pending_choice().unwrap().is_none());
        }
        quick_pass(&mut game);
    }
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["objects"]["source"]["acted"], true);
    assert_eq!(s["P2"]["leader"]["life"], 18_u8);
}

#[test]
fn bp06_114_without_the_named_arena_pays_its_printed_three_pp() {
    let mut game = game(&position("BP06-114"));
    play(&mut game, "source");
    finish_checks(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["pp"]["current"], 7_u8);
    assert_eq!(s["objects"]["source"]["power"], 3_u8);
}

#[test]
fn bp06_076_banishes_two_yokai_cards_for_effect_evolution_without_spending_normal_evolution() {
    let mut initial = position("BP06-076");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"ally","card":"BP06-084"}]);
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"fee1","card":"BP06-084"},{"id":"fee2","card":"BP06-084"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"form","card":"BP06-077"},{"id":"other-form","card":"BP06-085"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["pp"]["current"], 7_u8);
    assert_eq!(s["P1"]["cemetery"], json!([]));
    assert_eq!(s["objects"]["source"]["power"], 2_u8);
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"ally","evolve_card":"other-form","pay":{"pp":1_u8}})
        )
        .unwrap()
        .outcome,
        "resolved"
    );
}

#[test]
fn bp06_077_real_evolution_grants_storm_only_to_another_allied_yokai_follower() {
    let mut initial = evolution("BP06-076", "BP06-077");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"id":"ally","card":"BP06-084","state":{"entered_this_turn":true}}),
            json!({"id":"wrong","card":"BP01-171","state":{"entered_this_turn":true}}),
        ]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"wrong","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "cannot-attack"
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"ally","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved"
    );
}

#[test]
fn bp06_085_real_evolution_discards_yokai_then_heals_and_draws_two() {
    let mut initial = evolution("BP06-084", "BP06-085");
    initial["players"]["P1"]["leader"]["life"] = json!(17_u8);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"fee","card":"BP06-084"}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["leader"]["life"], 18_u8);
    assert_eq!(s["P1"]["hand"], json!(["P1-first", "P1-second"]));
    assert_eq!(s["P1"]["cemetery"], json!(["fee"]));
    assert_eq!(s["P1"]["pp"]["current"], 9_u8);
}

#[test]
fn bp06_115_real_evolution_searches_a_challenger_card_not_the_first_deck_card() {
    let mut initial = evolution("BP06-114", "BP06-115");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"found","card":"BP06-073"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game, 1);
    trigger(&mut game);
    let s = game.projection(View::Referee).unwrap();
    assert_eq!(s["P1"]["hand"], json!(["found"]));
    assert_eq!(s["P1"]["pp"]["current"], 9_u8);
    assert_eq!(s["objects"]["source"]["power"], 4_u8);
}
