use super::ability_combinations::{activate_card, finish};
use super::*;

#[test]
fn chronos_schedules_an_extra_turn_and_is_banished_by_its_last_words() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"chronos","card":"GFD01-008EN"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP01-091"}]);
    let mut game = game(&initial);
    let play = game.play_internal_card("chronos", actor()).unwrap();
    assert!(matches!(play, CardPlay::Entered(_)));
    finish(&mut game, Some(play), &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "chronos"]));
    assert_eq!(
        state["semantic_state"]["turn_schedule"]["extra"],
        json!([{"player":"P1","count":1_u8}])
    );

    activate_card(&mut game, "BP01-091", 2, &[]);
    let after_destruction = game.projection(View::Referee).unwrap();
    assert_eq!(after_destruction["P1"]["banish"], json!(["chronos"]));
}

#[test]
fn filenes_fanfare_can_engage_multiple_followers_and_cannot_target_aura() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"filene","card":"GFD02-003EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"second","card":"BP01-037"},
        {"id":"shiva","card":"GFD02-008EN"}
    ]);
    let mut game = game(&initial);
    let play = game.play_internal_card("filene", actor()).unwrap();
    finish(&mut game, Some(play), &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["first"]["acted"], true);
    assert_eq!(state["objects"]["second"]["acted"], true);
    assert_eq!(state["objects"]["shiva"]["acted"], false);
}

#[test]
fn jafnhars_fanfare_evolves_it_and_resolves_the_printed_damage_ability() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"jafnhar","card":"GFD02-025EN"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"GFD02-026EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{
        "id":"target", "card":"BP01-037", "state":{"hp":5_u8,"max_hp":5_u8}
    }]);
    let mut game = game(&initial);
    let play = game.play_internal_card("jafnhar", actor()).unwrap();
    finish(&mut game, Some(play), &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["target"]["hp"], 1_u8);
    assert_eq!(state["objects"]["jafnhar"]["evolved_with"], "form");
}

#[test]
fn shiva_counts_the_amulet_flipped_to_pay_its_end_phase_ability() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"shiva","card":"GFD02-008EN"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"follower","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut evolve_deck = (0_u8..4)
        .map(|n| json!({"id":format!("open-{n}"),"card":"GFD02-026EN","face_up":true}))
        .collect::<Vec<_>>();
    evolve_deck.push(json!({"id":"amulet","card":"BP08-090"}));
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!(evolve_deck);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    finish(&mut game, None, &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["amulet"]["face_up"], true);
    assert_eq!(state["P2"]["leader"]["life"], 16_u8);
    assert_eq!(state["P2"]["field"], json!(["follower"]));
}
