#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these game scenarios."
)]
use core::iter::once;

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
        } else if matches!(&question.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![ModeId(1)])
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

fn evolve(game: &mut Game, form: &str, cost: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":form,"pay":{"pp":cost}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "the real base uses its printed evolution cost"
    );
    triggers(game);
}
fn evolved_position(base: &str, form: &str) -> Value {
    let mut initial = position(base);
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}
fn activate(game: &mut Game, card: &str) {
    let id = selection().current[card]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    let step = game.activate_internal_card("source", id, actor()).unwrap();
    finish(game, step);
    game.resume_internal_card().unwrap();
    triggers(game);
}

fn named(state: &Value, zone: &str, name: &str) -> usize {
    state["P1"][zone]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}
fn end_turn(game: &mut Game) {
    let active = game.projection(View::Referee).unwrap()["turn"]["active"].clone();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"end-phase"}))
            .unwrap()
            .outcome,
        "resolved",
        "the active player ends its main phase"
    );
    loop {
        triggers(game);
        if game.projection(View::Referee).unwrap()["turn"]["active"] != active {
            break;
        }
        if game.internal_quick_choice().unwrap().is_some() {
            pass_quick(game);
        } else {
            assert_eq!(
                game.internal_rule_action(&json!({"do":"guard-act","select":[]}))
                    .unwrap()
                    .outcome,
                "resolved",
                "standing Ward followers may remain standing at the end-phase choice"
            );
        }
    }
}

#[test]
fn bp15_p29_opponents_grave_ten_adds_two_leader_damage() {
    for grave in [9_usize, 10] {
        let mut initial = position("BP15-P29");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP15-P29"}]);
        initial["players"]["P2"]["zones"]["cemetery"] = json!(
            (0..grave)
                .map(|n| json!({"id":format!("old{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
        assert_eq!(
            state["P2"]["leader"]["life"],
            if grave == 10 { 18_u8 } else { 20_u8 }
        );
        assert_eq!(state["P1"]["cemetery"], json!([]));
    }
}
#[test]
fn bp15_052_real_evolution_discards_spell_for_enemy_only_area_damage() {
    let mut initial = evolved_position("BP15-051", "BP15-052");
    initial["players"]["P1"]["zones"]["hand"] =
        json!([{"id":"spell","card":"BP01-023"},{"id":"wrong","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"small","card":"BP01-171"}));
    let mut game = game(&initial);
    evolve(&mut game, "form", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["wrong"]));
    assert_eq!(state["P1"]["cemetery"], json!(["spell"]));
    assert_eq!(state["P2"]["cemetery"], json!(["small"]));
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["objects"]["source"]["damage"], 0_u8);
}
#[test]
fn bp15_066_awakened_fanfare_self_damage_triggers_three_enemy_damage() {
    for awakened in [false, true] {
        let mut initial = position("BP15-066");
        initial["players"]["P1"]["pp"] = if awakened {
            json!({"current":10_u8,"max":10_u8})
        } else {
            json!({"current":6_u8,"max":6_u8})
        };
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["damage"], u8::from(awakened));
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if awakened { 3_u8 } else { 0_u8 }
        );
    }
}
#[test]
fn bp15_067_real_evolution_self_damage_triggers_its_own_damage_ability() {
    let mut game = game(&evolved_position("BP15-066", "BP15-067"));
    evolve(&mut game, "form", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["damage"], 1_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
}
#[test]
fn bp15_054_fanfare_requires_three_idol_cards_including_itself() {
    for others in [1_usize, 2] {
        let mut initial = position("BP15-054");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..others)
                .map(|n| json!({"id":format!("idol{n}"),"card":"BP15-054"}))
                .chain(once(json!({"id":"wrong","card":"BP01-171"})))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if others == 2 { 3_u8 } else { 0_u8 }
        );
    }
}
#[test]
fn bp15_pr13_non_awakened_token_damages_all_followers_but_no_leaders() {
    let mut initial = position("BP15-PR13");
    initial["players"]["P1"]["pp"] = json!({"current":6_u8,"max":6_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP15-PR13"}]);
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"ally","card":"BP01-037"},{"id":"own-small","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"opp-small","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["ally"]["damage"], 1_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    assert_eq!(state["objects"]["own-small"]["damage"], 1_u8);
    assert_eq!(state["objects"]["opp-small"]["damage"], 1_u8);
    assert_eq!(state["P1"]["cemetery"], json!([]));
    assert_eq!(state["P2"]["cemetery"], json!([]));
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
    assert_eq!(state["P2"]["leader"]["life"], 20_u8);
}
#[test]
fn bp15_023_attack_area_damage_requires_five_own_followers() {
    for others in [3_usize, 4] {
        let mut initial = position("BP15-023");
        initial["players"]["P1"]["zones"]["field"] = json!(
            (0..others)
                .map(|n| json!({"id":format!("ally{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            "resolved",
            "printed Storm allows the entry-turn attack"
        );
        triggers(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
            if others == 4 { 2_u8 } else { 0_u8 }
        );
    }
}
#[test]
fn bp15_071_real_evolution_damages_both_fields_including_itself() {
    let mut initial = evolved_position("BP15-070", "BP15-071");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-171"}));
    let mut game = game(&initial);
    evolve(&mut game, "form", 3);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P2"]["field"], json!([]));
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("source")),
        "the evolved source takes its own all-follower damage"
    );
    assert!(
        state["P1"]["cemetery"]
            .as_array()
            .unwrap()
            .contains(&json!("ally")),
        "the other own follower takes the all-follower damage"
    );
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
}
#[test]
fn bp15_011_evolution_damages_returns_real_base_to_hand_then_recovers_pp() {
    let mut initial = evolved_position("BP15-010", "BP15-011");
    initial["players"]["P1"]["pp"] = json!({"current":5_u8,"max":10_u8});
    let mut game = game(&initial);
    evolve(&mut game, "form", 1);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P1"]["field"], json!([]));
    assert_eq!(state["P1"]["hand"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}
#[test]
fn bp15_036_fanfare_draw_requires_a_commander_card() {
    for commander in [false, true] {
        let mut initial = position("BP15-036");
        initial["players"]["P1"]["zones"]["field"] =
            json!([{"id":"ally","card":if commander {"BP01-037"}else{"BP01-171"}}]);
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["hand"],
            if commander {
                json!(["first"])
            } else {
                json!([])
            }
        );
    }
}
#[test]
fn bp15_092_sacrifices_other_follower_then_destroys_heals_mills_and_draws() {
    let mut initial = position("BP15-092");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"material","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["cemetery"],
        json!(["material", "first", "second"])
    );
    assert_eq!(state["P1"]["hand"], json!(["third"]));
    assert_eq!(state["P1"]["leader"]["life"], 21_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["field"], json!(["source"]));
}
#[test]
fn bp15_110_sacrifices_amulet_to_add_entry_turn_storm() {
    let mut initial = position("BP15-110");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"amulet","card":"BP01-016"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["cemetery"],
        json!(["amulet"])
    );
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "the sacrificed amulet pays for Storm"
    );
}
#[test]
fn bp15_037_fanfare_acts_two_opposing_followers_at_most() {
    let mut initial = position("BP15-037");
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"enemy2","card":"BP01-037"}));
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"enemy3","card":"BP01-037"}));
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    assert_eq!(state["objects"]["enemy2"]["acted"], true);
    assert_eq!(state["objects"]["enemy3"]["acted"], false);
}

#[test]
fn bp15_p30_opponents_grave_ten_replaces_one_healing_with_two() {
    for grave in [9_usize, 10] {
        let mut initial = position("BP15-P30");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP15-P30"}]);
        initial["players"]["P2"]["zones"]["cemetery"] = json!(
            (0..grave)
                .map(|n| json!({"id":format!("old{n}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            if grave == 10 { 22_u8 } else { 21_u8 }
        );
    }
}
#[test]
fn bp15_pr17_token_destroys_and_only_named_mazel_ally_adds_leader_damage() {
    for mazel in [false, true] {
        let mut initial = position("BP15-PR17");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP15-PR17"}]);
        if mazel {
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"mazel","card":"BP15-112"}]);
        }
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(
            state["P2"]["leader"]["life"],
            if mazel { 18_u8 } else { 20_u8 }
        );
        assert_eq!(state["P1"]["cemetery"], json!([]));
    }
}
#[test]
fn bp15_055_discount_counts_wizards_before_entry_and_fanfare_draws_then_discards() {
    for two in [false, true] {
        let mut initial = position("BP15-055");
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"wizard","card":"BP02-046"},{"id":"other","card":if two {"BP08-042"}else{"BP01-171"}}]);
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], if two { 9_u8 } else { 7_u8 });
        assert_eq!(state["P1"]["hand"], json!([]));
        assert_eq!(state["P1"]["cemetery"], json!(["first"]));
    }
}
#[test]
fn bp15_072_awakened_fanfare_self_damage_triggers_top_one_absolute_selection() {
    for awakened in [false, true] {
        let mut initial = position("BP15-072");
        initial["players"]["P1"]["pp"] = if awakened {
            json!({"current":10_u8,"max":10_u8})
        } else {
            json!({"current":6_u8,"max":6_u8})
        };
        initial["players"]["P1"]["zones"]["deck"] =
            json!([{"id":"top","card":"BP15-094"},{"id":"tail","card":"BP01-171"}]);
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["source"]["damage"], u8::from(awakened));
        assert_eq!(
            state["P1"]["hand"],
            if awakened { json!(["top"]) } else { json!([]) }
        );
        assert_eq!(
            state["P1"]["deck"],
            if awakened {
                json!(["tail"])
            } else {
                json!(["top", "tail"])
            }
        );
    }
}
#[test]
fn bp15_049_selects_idol_from_top_three_and_orders_remainder() {
    let mut initial = position("BP15-049");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"wrong","card":"BP01-171"},{"id":"idol","card":"BP15-049"},{"id":"rest","card":"BP01-037"},{"id":"tail","card":"BP01-171"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["idol"]));
    assert_eq!(state["P1"]["deck"], json!(["tail", "rest", "wrong"]));
}
#[test]
fn bp15_026_damages_enemy_and_places_same_batch_cup_token_in_ex() {
    let mut game = game(&position("BP15-026"));
    play(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 2_u8);
    assert_eq!(named(&state, "ex", "黄金の杯"), 1);
}
#[test]
fn bp15_112_discards_three_for_proof_and_banishes_named_mazel_once_per_turn() {
    let mut initial = position("BP15-112");
    for n in 0_u8..3 {
        initial["players"]["P1"]["zones"]["hand"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":format!("payer{n}"),"card":"BP01-171"}));
    }
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"old1","card":"BP05-103"},{"id":"old2","card":"BP05-103"}]);
    let mut game = game(&initial);
    play(&mut game);
    triggers(&mut game);
    activate(&mut game, "BP15-112");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, "ex", "絶大の証明"), 1);
    assert_eq!(state["P1"]["hand"], json!(["first", "second"]));
    assert_eq!(state["P1"]["banish"], json!(["old1"]));
    assert_eq!(state["P1"]["cemetery"].as_array().unwrap().len(), 4);
    assert_eq!(state["P2"]["leader"]["life"], 18_u8);
    let id = selection().current["BP15-112"]
        .program
        .card()
        .abilities
        .iter()
        .find(|a| a.node.kind == AbilityKind::Activated)
        .unwrap()
        .node
        .id;
    assert!(
        game.activate_internal_card("source", id, actor()).is_err(),
        "another named grave material does not bypass the printed once-per-turn limit"
    );
}
#[test]
fn bp15_094_fanfare_counts_opposing_followers_and_opponents_main_phase_heals() {
    for followers in [2_usize, 3] {
        let mut initial = position("BP15-094");
        initial["players"]["P2"]["zones"]["field"] = json!(
            (0..followers)
                .map(|n| json!({"id":format!("enemy{n}"),"card":"BP01-037"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            named(&before, "ex", "絶望の奔流"),
            usize::from(followers == 3)
        );
        end_turn(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
            22_u8
        );
    }
}
#[test]
fn bp15_095_real_evolution_destroys_every_other_follower_and_keeps_phase_healing() {
    let mut initial = evolved_position("BP15-094", "BP15-095");
    initial["players"]["P1"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"ally","card":"BP01-171"}));
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"enemy2","card":"BP01-037"}));
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"oppdraw","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game, "form", 3);
    let before = game.projection(View::Referee).unwrap();
    assert_eq!(before["P1"]["field"], json!(["source"]));
    assert_eq!(before["P1"]["cemetery"], json!(["ally"]));
    assert_eq!(before["P2"]["field"], json!([]));
    end_turn(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        22_u8
    );
}
#[test]
fn bp15_017_creates_wisp_then_acts_and_banishes_ex_material_to_return_enemy() {
    let initial = position("BP15-017");
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "bp15-ex-material",
    )
    .unwrap();
    play(&mut game);
    triggers(&mut game);
    assert_eq!(
        named(
            &game.projection(View::Referee).unwrap(),
            "ex",
            "フェアリーウィスプ"
        ),
        1
    );
    activate(&mut game, "BP15-017");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["objects"]["source"]["acted"], true);
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["hand"], json!(["enemy"]));
}
#[test]
fn bp15_009_recovers_named_amatsu_follower_from_grave_to_field() {
    let mut initial = position("BP15-009");
    initial["players"]["P1"]["zones"]["cemetery"] =
        json!([{"id":"wrong","card":"BP01-171"},{"id":"returned","card":"BP15-002"}]);
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["returned"]));
    assert_eq!(state["P1"]["cemetery"], json!(["wrong", "source"]));
}
#[test]
fn bp15_089_real_evolution_revives_rulunai_or_damages_and_lastword_mills_one() {
    for mode in [ModeId(1), ModeId(2)] {
        let mut initial = evolved_position("BP15-088", "BP15-089");
        initial["players"]["P1"]["zones"]["cemetery"] = json!([{"id":"rulunai","card":"BP20-074"}]);
        initial["players"]["P2"]["zones"]["field"][0]["state"] = json!({"acted":true});
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "the real base evolves for one PP"
        );
        let pending = game.internal_pending_choice().unwrap().unwrap();
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("evolution ability")
        };
        let CardPlay::Awaiting(question) = game
            .answer_internal_card(
                pending.actor,
                pending.token,
                Answer::Pending(options[0].handle),
            )
            .unwrap()
        else {
            panic!("mode choice")
        };
        assert!(
            matches!(&question.domain, InputDomain::Modes { .. }),
            "the evolved follower offers its printed modes"
        );
        let step = game
            .answer_internal_card(question.actor, question.token, Answer::Modes(vec![mode]))
            .unwrap();
        finish(&mut game, step);
        game.resume_internal_card().unwrap();
        let before = game.projection(View::Referee).unwrap();
        assert_eq!(
            before["objects"]["enemy"]["damage"],
            if mode == ModeId(2) { 2_u8 } else { 0_u8 }
        );
        assert_eq!(
            before["P1"]["field"]
                .as_array()
                .unwrap()
                .contains(&json!("rulunai")),
            mode == ModeId(1)
        );
        if mode == ModeId(2) {
            assert_eq!(
                game.internal_rule_action(
                    &json!({"do":"attack","attacker":"source","target":"enemy"})
                )
                .unwrap()
                .outcome,
                "resolved",
                "the evolved follower legally attacks the rested enemy"
            );
            pass_quick(&mut game);
            triggers(&mut game);
            let after = game.projection(View::Referee).unwrap();
            assert!(
                after["P1"]["cemetery"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("first")),
                "the Lastword puts the real top card into the cemetery"
            );
            assert_eq!(after["P1"]["deck"], json!(["second", "third", "fourth"]));
        }
    }
}

#[test]
fn bp15_047_only_entry_outside_hand_evolves_into_the_real_admitted_form() {
    for from_ex in [false, true] {
        let mut initial = position("BP15-047");
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP15-048"}]);
        if from_ex {
            initial["players"]["P1"]["zones"]["hand"] = json!([]);
            initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP15-047"}]);
        }
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P1"]["evolution"],
            if from_ex { json!(["form"]) } else { json!([]) }
        );
        assert_eq!(
            state["objects"]["enemy"]["damage"],
            if from_ex { 2_u8 } else { 0_u8 }
        );
        assert_eq!(
            state["P1"]["pp"]["current"], 8_u8,
            "effect evolution does not charge the printed normal evolution payment"
        );
    }
}
