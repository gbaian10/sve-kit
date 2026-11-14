use super::tier0_b::{destruction_game, evolve, game, play, position, settle};
use super::*;

#[test]
fn cp04_072_entry_draws_for_both_players_and_departure_hits_the_opponent() {
    let mut board = destruction_game(&position("CP04-072"));
    play(&mut board, "source");
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["draw"]));
    assert_eq!(state["P2"]["hand"], json!(["their-draw"]));
    activate(&mut board, "P1");
    settle(&mut board);
    let after = board.projection(View::Referee).unwrap();
    assert_eq!(after["P2"]["leader"]["life"], 19_u8);
}

#[test]
fn ecp02_019_fanfare_draws_one_card_for_each_player() {
    let mut board = game(&position("ECP02-019"));
    play(&mut board, "source");
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["draw"]));
    assert_eq!(state["P2"]["hand"], json!(["their-draw"]));
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn sd03_011_evolution_draws_one_card_for_each_player() {
    let mut initial = position("SD03-010");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"SD03-010"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"SD03-011"}]);
    let mut board = game(&initial);
    assert_eq!(
        board
            .internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}})
            )
            .unwrap()
            .outcome,
        "resolved"
    );
    settle(&mut board);
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["draw"]));
    assert_eq!(state["P2"]["hand"], json!(["their-draw"]));
}

#[test]
fn cp01_080_reveals_six_places_selected_uma_followers_and_takes_the_remainder() {
    let mut initial = position("CP01-080");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"uma-a","card":"CP01-022"},
        {"id":"uma-b","card":"CP01-023"},
        {"id":"rest-a","card":"BP01-072"},
        {"id":"rest-b","card":"BP01-072"},
        {"id":"rest-c","card":"BP01-072"},
        {"id":"rest-d","card":"BP01-072"}
    ]);
    let mut board = game(&initial);
    play(&mut board, "source");
    let state = board.projection(View::Referee).unwrap();
    let mut field = state["P1"]["field"].as_array().unwrap().clone();
    field.sort_by_key(|id| id.as_str().unwrap().to_owned());
    assert_eq!(field, vec![json!("uma-a"), json!("uma-b")]);
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 4_usize);
    assert_eq!(state["P1"]["deck"], json!([]));
    assert_eq!(state["P1"]["cemetery"], json!(["source"]));
    assert_eq!(state["P1"]["pp"]["current"], 0_u8);
}

#[test]
fn cp02_037_captures_the_selected_ex_cost_then_plays_it_free_and_uses_its_activation() {
    let mut initial = position("CP02-036");
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"discard","card":"BP01-171"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"CP02-036"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"CP02-037"}]);
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"candidate","card":"BP01-037"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut board = game(&initial);
    evolve(&mut board, "form", 2);
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["leader"]["life"],
        15_u8
    );
    play(&mut board, "candidate");
    assert_eq!(
        board.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        8_u8
    );
    tier0_b::activate(&mut board, "CP02-037");
    let state = board.projection(View::Referee).unwrap();
    assert_eq!(state["P2"]["cemetery"], json!(["enemy"]));
    assert_eq!(state["P1"]["cemetery"], json!(["discard"]));
    assert_eq!(state["objects"]["source"]["acted"], true);
}
