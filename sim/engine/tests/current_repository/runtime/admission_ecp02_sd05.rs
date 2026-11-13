use super::admission_fixture::{enter, finish, position};
use super::*;

#[test]
fn sd05_005_creates_one_bat_and_grants_it_drain_from_the_live_source() {
    let mut game = game(&position("SD05-005"));
    enter(&mut game);
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    assert_eq!(field.len(), 2);
    let bat = field
        .iter()
        .find(|id| id.as_str() != Some("source"))
        .unwrap()
        .as_str()
        .unwrap();
    assert!(
        state["objects"][bat]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("ドレイン"))
    );
}

#[test]
fn ecp02_039_whole_activation_pays_three_distinct_traits_then_heals_draws_and_recovers_pp() {
    let mut initial = position("ECP02-039");
    initial["players"]["P1"]["pp"]["current"] = json!(0_i32);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"ECP02-039"}]);
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"cute","card":"ECP02-010"},{"id":"cool","card":"ECP02-001"},{"id":"passion","card":"ECP02-008"}]);
    let mut game = game(&initial);
    let ability = game.internal_activated_sources("source").unwrap()[0].definition;
    let CardPlay::Awaiting(q) = game
        .activate_internal_card("source", ability, PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("three trait payment");
    };
    assert!(matches!(q.domain, InputDomain::CostMaterials(_)));
    let before = game.projection(View::Referee).unwrap();
    game.answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0, 0, 0]))
        .unwrap_err();
    assert_eq!(game.projection(View::Referee).unwrap(), before);
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::CostMaterials(vec![0, 1, 2]))
        .unwrap();
    finish(&mut game, play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["cemetery"], json!(["cute", "cool", "passion"]));
    assert_eq!(state["P1"]["hand"].as_array().unwrap().len(), 3);
    assert_eq!(state["P1"]["leader"]["life"], 13_i32);
    assert_eq!(state["P1"]["pp"]["current"], 3_i32);
    assert_eq!(state["objects"]["source"]["acted"], true);
}
