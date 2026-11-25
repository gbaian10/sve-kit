use super::ability_combinations::{evolve, evolve_position, finish, loaded, play, position, view};
use super::*;

fn earth_deck() -> Value {
    json!([
        {"id":"spent-one","card":"CP05-092","face_up":true},
        {"id":"spent-two","card":"CP05-092","face_up":true},
        {"id":"spent-three","card":"CP05-092","face_up":true},
        {"id":"ready","card":"CP05-092"}
    ])
}

#[test]
fn galleon_can_reveal_a_fourth_earth_evolution_and_then_draw() {
    let mut initial = position("CP05-091", "hand");
    initial["players"]["P1"]["zones"]["evolve_deck"] = earth_deck();
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"top","card":"BP01-171"}]);
    let mut game = loaded(&initial);
    play(&mut game, &[]);
    let state = view(&game);
    assert_eq!(state["objects"]["ready"]["face_up"], true);
    assert_eq!(state["P1"]["hand"], json!(["top"]));
}

#[test]
fn galleon_super_evolution_rounds_up_the_three_face_up_earth_cards() {
    let mut initial = position("CP05-091", "field");
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["sep"] = json!(1_u8);
    initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
    initial["players"]["P1"]["zones"]["evolve_deck"] = earth_deck();
    let mut game = loaded(&initial);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"ready","pay":{"pp":1_u8,"sep":1_u8}}),
    )
    .unwrap();
    finish(&mut game, None, &[]);
    let state = view(&game);
    assert_eq!(state["P1"]["leader"]["life"], 12_u8);
    let materials = state["P1"]["ex"].as_array().unwrap();
    assert_eq!(materials.len(), 2);
    for id in materials {
        assert_eq!(state["objects"][id.as_str().unwrap()]["card"], "CP05-T11");
    }
}

#[test]
fn vira_evolution_damages_by_revealed_light_count_and_keeps_one_card() {
    let mut initial = evolve_position("synthetic-evolution", "CP05-094");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"kept","card":"CP05-090"},
        {"id":"left","card":"CP05-090"},
        {"id":"off-element","card":"BP01-037"},
        {"id":"unseen","card":"BP01-171"}
    ]);
    let mut game = super_evolution_pp_cost::synthetic_base("CP05-094", &initial);
    evolve(&mut game, 1);
    let state = view(&game);
    assert_eq!(state["objects"]["enemy"]["hp"], 28_u8);
    assert_eq!(state["P1"]["hand"], json!(["kept"]));
    assert_eq!(
        state["P1"]["deck"],
        json!(["unseen", "left", "off-element"])
    );
}
