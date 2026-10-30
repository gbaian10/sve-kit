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
    initial["players"]["P1"]["zones"]["field"] = json!([]);
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
fn named(state: &Value, name: &str) -> usize {
    state["P1"]["ex"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|id| state["objects"][id.as_str().unwrap()]["name"] == name)
        .count()
}

#[test]
fn bp14_p31_token_spell_checks_awakened_before_damage() {
    for awakened in [false, true] {
        let mut initial = position("BP14-P31");
        initial["players"]["P1"]["zones"]["hand"] = json!([]);
        initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"source","card":"BP14-P31"}]);
        initial["players"]["P1"]["pp"] = if awakened {
            json!({"current":10_u8,"max":10_u8})
        } else {
            json!({"current":6_u8,"max":6_u8})
        };
        let mut game = game(&initial);
        play(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            state["P2"]["field"],
            if awakened {
                json!([])
            } else {
                json!(["enemy"])
            }
        );
        if !awakened {
            assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
        }
        assert_eq!(state["P1"]["cemetery"], json!([]));
        assert_eq!(state["P1"]["ex"], json!([]));
    }
}
#[test]
fn bp14_083_evolution_deals_four_to_enemy_and_leader_then_discards() {
    let mut initial = evolved_position("BP14-082", "BP14-083");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"discarded","card":"BP01-171"}]);
    let mut game = game(&initial);
    evolve(&mut game, "form", 5);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["discarded"]));
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
}
#[test]
fn bp14_080_hand_size_gates_pp_recovery_and_extra_leader_damage_separately() {
    for remaining in [0_usize, 2, 3] {
        let mut initial = position("BP14-080");
        for n in 0..remaining {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("held{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["objects"]["enemy"]["damage"], 3_u8);
        assert_eq!(
            state["P1"]["pp"]["current"],
            if remaining <= 2 { 8_u8 } else { 6_u8 }
        );
        assert_eq!(
            state["P2"]["leader"]["life"],
            if remaining == 0 { 18_u8 } else { 20_u8 }
        );
    }
}
#[test]
fn bp14_051_draws_two_before_computing_opponent_only_area_damage() {
    let mut initial = position("BP14-051");
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"held1","card":"BP01-171"}));
    initial["players"]["P1"]["zones"]["hand"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"held2","card":"BP01-171"}));
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"small","card":"BP01-171"}));
    let mut game = game(&initial);
    play(&mut game);
    game.resume_internal_card().unwrap();
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P1"]["hand"],
        json!(["held1", "held2", "first", "second"])
    );
    assert_eq!(state["objects"]["enemy"]["damage"], 4_u8);
    assert_eq!(state["P2"]["cemetery"], json!(["small"]));
    assert_eq!(state["objects"]["own"]["damage"], 0_u8);
}
#[test]
fn bp14_056_acts_at_most_two_enemy_followers() {
    let mut initial = position("BP14-056");
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
fn bp14_085_empty_hand_adds_storm_but_two_cards_only_add_power() {
    for remaining in [0_usize, 2, 3] {
        let mut initial = position("BP14-085");
        for n in 0..remaining {
            initial["players"]["P1"]["zones"]["hand"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":format!("held{n}"),"card":"BP01-171"}));
        }
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        assert_eq!(
            game.projection(View::Referee).unwrap()["objects"]["source"]["power"],
            if remaining <= 2 { 3_u8 } else { 2_u8 }
        );
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"attack","attacker":"source","target":"P2.leader"})
            )
            .unwrap()
            .outcome,
            if remaining == 0 {
                "resolved"
            } else {
                "cannot-attack"
            }
        );
    }
}
#[test]
fn bp14_032_current_storm_can_attack_leader_on_entry_turn() {
    let mut game = game(&position("BP14-032"));
    play(&mut game);
    game.resume_internal_card().unwrap();
    assert_eq!(
        game.internal_rule_action(&json!({"do":"attack","attacker":"source","target":"P2.leader"}))
            .unwrap()
            .outcome,
        "resolved",
        "printed Storm permits the entry-turn leader attack"
    );
}
#[test]
fn bp14_097_two_paid_effects_move_enemy_followers_to_their_owners_ex() {
    let mut initial = position("BP14-097");
    initial["players"]["P1"]["zones"]["ex"] =
        json!([{"id":"fee1","card":"BP01-171"},{"id":"fee2","card":"BP01-171"}]);
    initial["players"]["P2"]["zones"]["field"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"enemy2","card":"BP01-037"}));
    let mut game = Game::from_selected_effects(
        selection_with_rules().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "bp14-ex-material",
    )
    .unwrap();
    play(&mut game);
    triggers(&mut game);
    activate(&mut game, "BP14-097");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["ex"], json!([]));
    assert_eq!(state["P1"]["banish"], json!(["fee1", "fee2"]));
    assert_eq!(state["P2"]["field"], json!([]));
    assert_eq!(state["P2"]["ex"], json!(["enemy", "enemy2"]));
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
}
#[test]
fn bp14_053_creates_pressure_token_and_heals_only_when_awakened() {
    for awakened in [false, true] {
        let mut initial = position("BP14-053");
        initial["players"]["P1"]["pp"] = if awakened {
            json!({"current":10_u8,"max":10_u8})
        } else {
            json!({"current":6_u8,"max":6_u8})
        };
        let mut game = game(&initial);
        play(&mut game);
        triggers(&mut game);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(named(&state, "波濤の暴圧"), 1);
        assert_eq!(
            state["P1"]["leader"]["life"],
            if awakened { 22_u8 } else { 20_u8 }
        );
    }
}
#[test]
fn bp14_095_creates_fox_in_ex_without_triggering_its_field_fanfare() {
    let mut game = game(&position("BP14-095"));
    play(&mut game);
    triggers(&mut game);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(named(&state, "招来の狐"), 1);
    assert_eq!(state["P1"]["leader"]["life"], 20_u8);
}
