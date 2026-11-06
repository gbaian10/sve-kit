#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn position(card: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    for seat in ["P1", "P2"] {
        initial["players"][seat]["zones"]["deck"] = json!([
            {"id":format!("{seat}-first"),"card":"BP01-171"},
            {"id":format!("{seat}-second"),"card":"BP01-171"}
        ]);
    }
    initial
}

fn finish(game: &mut Game, step: CardPlay) {
    finish_mode(game, step, ModeId(1));
}

fn finish_mode(game: &mut Game, mut step: CardPlay, mode: ModeId) {
    while let CardPlay::Awaiting(q) = step {
        let answer = if let InputDomain::Targets(domain) = &q.domain {
            let mut members: Vec<_> = domain.members.iter().collect();
            members.sort_by_key(|member| {
                if member.card.as_deref() == Some("BP01-092") {
                    0_u8
                } else if member.card.as_deref() == Some("BP01-037") {
                    2
                } else {
                    3
                }
            });
            let limit = domain
                .cardinality
                .maximum
                .map_or(members.len(), |n| usize::try_from(n.0).unwrap());
            Answer::Targets(members.into_iter().take(limit).map(|m| m.handle).collect())
        } else if let InputDomain::CostMaterials(domain) = &q.domain {
            let members: Vec<_> = domain.members.iter().collect();
            assert!(
                members.iter().all(|member| member.player == actor()),
                "Only the controller can supply these cost materials"
            );
            let limit = domain
                .cardinality
                .maximum
                .map_or(members.len(), |n| usize::try_from(n.0).unwrap());
            Answer::CostMaterials(members.into_iter().take(limit).map(|m| m.handle).collect())
        } else if let InputDomain::Search(domain) = &q.domain {
            let limit = usize::try_from(
                domain
                    .cardinality
                    .maximum
                    .unwrap_or(domain.cardinality.minimum)
                    .0,
            )
            .unwrap();
            Answer::Search(SearchAnswer::Ungrouped(
                domain.cards.iter().take(limit).map(|c| c.handle).collect(),
            ))
        } else if let InputDomain::Order(cards) = &q.domain {
            Answer::Order(cards.iter().rev().map(|card| card.handle).collect())
        } else if matches!(&q.domain, InputDomain::Modes { .. }) {
            Answer::Modes(vec![mode])
        } else if matches!(&q.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(&q.domain, InputDomain::TokenCapacity { .. }) {
            Answer::TokenCapacity(0)
        } else if matches!(&q.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else {
            panic!("unexpected decision")
        };
        step = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

fn checks(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
}

fn play(game: &mut Game, id: &str) {
    let step = game.play_internal_card(id, actor()).unwrap();
    finish(game, step);
    checks(game);
}

fn trigger(game: &mut Game, card: &str) {
    trigger_mode(game, card, ModeId(1));
}

fn trigger_mode(game: &mut Game, card: &str, mode: ModeId) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("expected a pending ability")
    };
    let chosen = options.iter().find(|option| option.card == card).unwrap();
    let step = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(chosen.handle))
        .unwrap();
    finish_mode(game, step, mode);
    checks(game);
}

fn keyword(state: &Value, object: &str, word: &str) -> bool {
    state["objects"][object]["keywords"]
        .as_array()
        .unwrap()
        .contains(&json!(word))
}

fn quick_pass(game: &mut Game) {
    for _ in 0_u8..4 {
        let Some(q) = game.internal_quick_choice().unwrap() else {
            return;
        };
        game.answer_internal_card(q.actor, q.token, Answer::Quick(None))
            .unwrap();
    }
    panic!("The Quick window must finish");
}

#[test]
fn bp14_102_creates_only_one_falcon_per_turn_from_healing_and_acts_an_enemy_on_its_entry() {
    let mut initial = position("BP14-102");
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"repair1","card":"BP07-T02"},{"id":"repair2","card":"BP07-T02"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    play(&mut game, "repair1");
    trigger(&mut game, "BP14-102");
    trigger(&mut game, "BP14-102");
    play(&mut game, "repair2");
    assert!(
        game.internal_pending_choice().unwrap().is_none(),
        "A second heal in this turn must not create another Falcon"
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2_usize);
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    assert_eq!(state["objects"]["enemy"]["acted"], true);
    let falcon = state["P1"]["field"][1].as_str().unwrap();
    assert!(keyword(&state, falcon, "疾走"));
}

#[test]
fn bp14_104_creates_two_falcons_and_buffs_each_from_its_entry_trigger() {
    let mut game = game(&position("BP14-104"));
    play(&mut game, "source");
    trigger(&mut game, "BP14-104");
    trigger(&mut game, "BP14-104");
    trigger(&mut game, "BP14-104");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 3_usize);
    for token in state["P1"]["field"].as_array().unwrap().iter().skip(1) {
        let id = token.as_str().unwrap();
        assert_eq!(state["objects"][id]["power"], 3_u8);
        assert_eq!(state["objects"][id]["hp"], 3_u8);
    }
}

#[test]
fn bp08_113_takes_one_follower_and_one_amulet_from_seven_and_reveals_both() {
    let mut initial = position("BP08-113");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"follower","card":"BP01-034"},{"id":"amulet","card":"BP01-041"},{"id":"a","card":"BP01-116"},{"id":"b","card":"BP01-116"},{"id":"c","card":"BP01-116"},{"id":"d","card":"BP01-116"},{"id":"e","card":"BP01-116"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["follower", "amulet"]));
    assert_eq!(state["P1"]["deck"], json!(["e", "d", "c", "b", "a"]));
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|c| c["id"] == "follower"),
        "The chosen follower is revealed"
    );
    assert!(
        known.iter().any(|c| c["id"] == "amulet"),
        "The chosen amulet is revealed"
    );
    assert!(
        !known.iter().any(|c| c["id"] == "a"),
        "The unselected cards remain private"
    );
}

#[test]
fn bp16_046_creates_a_sigil_with_one_stack_on_entry() {
    let mut game = game(&position("BP16-046"));
    play(&mut game, "source");
    trigger(&mut game, "BP16-046");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 8_u8);
    let sigil = state["P1"]["field"][1].as_str().unwrap();
    assert_eq!(
        state["objects"][sigil]["counters"]["スタックカウンター"],
        1_u8
    );
}

#[test]
fn ecp02_069_uses_a_risa_on_the_field_for_keywords_and_a_magic_item() {
    let mut initial = position("ECP02-069");
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"risa","card":"CP02-093"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "ECP02-069");
    let state = game.projection(View::Referee).unwrap();
    assert!(keyword(&state, "source", "突進"));
    assert!(keyword(&state, "source", "指定攻撃"));
    assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 1_usize);
}

#[test]
fn bp13_046_pays_earth_rite_for_two_damage_batches_and_creates_a_sigil_on_death() {
    let mut initial = position("BP13-046");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"soil","card":"BP22-T10","state":{"counters":{"stack_counter":1_u8}}}]);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"kill","card":"BP01-116"}));
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP13-046");
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        4_u8
    );
    play(&mut game, "kill");
    trigger(&mut game, "BP13-046");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["source", "kill"]));
    let sigil = state["P1"]["field"][0].as_str().unwrap();
    assert_eq!(
        state["objects"][sigil]["counters"]["スタックカウンター"],
        1_u8
    );
}

#[test]
fn bp13_063_reveals_any_number_of_expensive_dragon_cards_from_five() {
    let mut initial = position("BP13-063");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"dragon1","card":"BP01-080"},{"id":"dragon2","card":"BP01-097"},{"id":"a","card":"BP01-116"},{"id":"b","card":"BP01-116"},{"id":"c","card":"BP01-116"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP13-063");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["dragon1", "dragon2"]));
    assert_eq!(state["P1"]["deck"], json!(["c", "b", "a"]));
    let public = game.projection(View::P2).unwrap();
    let known = public["known_cards"].as_array().unwrap();
    assert!(
        known.iter().any(|c| c["id"] == "dragon1"),
        "The first Dragon is revealed"
    );
    assert!(
        known.iter().any(|c| c["id"] == "dragon2"),
        "The second Dragon is revealed"
    );
}

#[test]
fn bp14_041_uses_current_power_on_attack_and_returns_as_a_new_two_two_storm_follower() {
    let mut initial = position("BP14-041");
    initial["players"]["P1"]["zones"]["field"] =
        json!([{"id":"soil","card":"BP22-T10","state":{"counters":{"stack_counter":1_u8}}}]);
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"kill","card":"BP01-116"}));
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"enemy"}))
            .unwrap()
            .outcome,
        "resolved",
        "Rush permits attacking this follower"
    );
    trigger(&mut game, "BP14-041");
    quick_pass(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P2"]["cemetery"],
        json!(["enemy"])
    );
    play(&mut game, "kill");
    trigger(&mut game, "BP14-041");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source"]));
    assert_eq!(state["objects"]["source"]["power"], 2_u8);
    assert_eq!(state["objects"]["source"]["hp"], 2_u8);
    assert!(keyword(&state, "source", "疾走"));
}

#[test]
fn bp14_062_damages_an_acted_enemy_at_end_phase_and_prevents_its_next_start_stand() {
    let mut initial = position("BP14-062");
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"acted":true}}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP14-062");
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    trigger(&mut game, "BP14-062");
    quick_pass(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["objects"]["enemy"]["acted"], true);
}

#[test]
fn bp14_103_puts_a_small_beast_in_ex_and_discounts_its_real_play() {
    let mut initial = position("BP14-103");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"picked","card":"BP01-021"},{"id":"a","card":"BP01-116"},{"id":"b","card":"BP01-116"},{"id":"c","card":"BP01-116"},{"id":"d","card":"BP01-116"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP14-103");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["picked"])
    );
    play(&mut game, "picked");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
    assert_eq!(state["P1"]["field"], json!(["source", "picked"]));
}

#[test]
fn bp12_066_searches_a_dragon_spell_to_ex_then_applies_its_play_discount() {
    let mut initial = position("BP12-066");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"BP01-098"}]);
    let mut game = game(&initial);
    play(&mut game, "source");
    trigger(&mut game, "BP12-066");
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["found"])
    );
    play(&mut game, "found");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 3_u8);
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
}
