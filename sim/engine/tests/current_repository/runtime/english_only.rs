use super::ability_combinations::finish;
use super::*;

#[test]
fn shining_bellringer_angel_offers_ward_and_draws_two_on_entry() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"angel","card":"GFE01-018EN"}]);
    initial["players"]["P1"]["zones"]["deck"] =
        json!([{"id":"first","card":"BP01-037"},{"id":"second","card":"BP01-037"}]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(ward) = game.play_internal_card("angel", actor()).unwrap() else {
        panic!("Ward offers entry posture");
    };
    assert!(matches!(ward.domain, InputDomain::WardEntry { .. }));
    let entered = game
        .answer_internal_card(ward.actor, ward.token, Answer::WardEntry(true))
        .unwrap();
    finish(&mut game, Some(entered), &[]);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["angel"]["acted"], true);
    assert_eq!(state["P1"]["hand"], json!(["first", "second"]));
    assert_eq!(state["P1"]["pp"]["current"], 5_u8);
}

#[test]
fn spectral_cascade_buries_five_then_draws_in_an_opponents_end_phase() {
    let mut initial = draw_position();
    initial["players"]["P2"]["zones"]["hand"] = json!([{"id":"cascade","card":"GFB01c-022EN"}]);
    let deck = ["one", "two", "three", "four", "five", "drawn"]
        .map(|id| json!({"id":id,"card":"BP01-037"}));
    initial["players"]["P2"]["zones"]["deck"] = json!(deck);
    let mut game = game(&initial);
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    let quick = game.internal_quick_choice().unwrap().unwrap();
    let InputDomain::Quick { options, .. } = &quick.domain else {
        panic!("end-of-turn Quick window");
    };
    assert_eq!(quick.actor, PlayerId { seat: 2 });
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].card, "GFB01c-022EN");
    resolved(
        game.answer_internal_card(
            quick.actor,
            quick.token,
            Answer::Quick(Some(options[0].handle)),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(
        state["P2"]["cemetery"],
        json!(["one", "two", "three", "four", "five", "cascade"])
    );
    assert_eq!(state["P2"]["hand"], json!(["drawn"]));
}
