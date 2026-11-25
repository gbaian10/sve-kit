#![expect(
    clippy::panic,
    reason = "An unexpected search decision invalidates the printed card scenarios."
)]

use super::ability_combinations::{
    activate_card, decision_answer, evolve_position, finish, loaded, position, view,
};
use super::*;

fn choose_only_search_candidate(game: &mut Game, play: CardPlay, number: &str) {
    let question = awaiting(play);
    let InputDomain::Search(domain) = &question.domain else {
        panic!("expected a search candidate list");
    };
    assert_eq!(
        domain.cards.len(),
        1,
        "the search must offer exactly one eligible follower"
    );
    assert_eq!(
        domain.cards[0].card, number,
        "the offered follower must meet the printed Quick and cost requirements"
    );
    let accepted = game
        .answer_internal_card(
            question.actor,
            question.token,
            decision_answer(&question.domain, &[]),
        )
        .unwrap();
    finish(game, Some(accepted), &[]);
}

#[test]
fn sophia_evolution_searches_a_low_cost_quick_activation_follower() {
    let mut initial = evolve_position("CP05-101", "CP05-102");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"quick-spell","card":"BP01-098"},
        {"id":"quick-amulet","card":"BP20-103"},
        {"id":"follower","card":"CP05-108"},
        {"id":"over-budget","card":"CP05-101"},
        {"id":"quick-follower","card":"CP05-111"}
    ]);
    let mut game = loaded(&initial);
    let action = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(action.outcome, "resolved");
    let search = resolve_pending_card(&mut game, "CP05-102");
    choose_only_search_candidate(&mut game, search, "CP05-111");
    assert!(
        view(&game)["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("quick-follower"))
    );
}

#[test]
fn katalina_fanfare_enters_a_cost_one_quick_follower_and_creates_water() {
    let mut initial = position("CP05-111", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"over-budget","card":"CP05-103"},
        {"id":"quick-spell","card":"BP01-098"},
        {"id":"quick-amulet","card":"BP20-103"},
        {"id":"eligible","card":"CP05-089"}
    ]);
    let mut game = loaded(&initial);
    let search = enter_with_fanfare(&mut game, "CP05-111");
    choose_only_search_candidate(&mut game, search, "CP05-089");
    let state = view(&game);
    assert!(
        state["P1"]["field"]
            .as_array()
            .unwrap()
            .contains(&json!("eligible"))
    );
    let id = state["P1"]["ex"][0].as_str().unwrap();
    assert_eq!(state["objects"][id]["card"], "CP05-T10");
}

#[test]
fn katalina_discards_itself_from_hand_to_create_water_and_draw() {
    let mut initial = position("CP05-111", "hand");
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = loaded(&initial);
    activate_card(&mut game, "CP05-111", 3, &[]);

    let state = view(&game);
    assert_eq!(state["P1"]["pp"]["current"], 9_u8);
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    let materials = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(materials.len(), 1);
    let id = materials[0].as_str().unwrap();
    assert_eq!(state["objects"][id]["card"], "CP05-T10");
}
