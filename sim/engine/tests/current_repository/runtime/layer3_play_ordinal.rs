use super::event_cemetery::finish_checks;
use super::union_burst_batch1::{finish, pending, question};
use super::*;
use sve_engine::game::internal::SearchAnswer;

#[test]
fn daiwa_searches_discounts_the_first_uma_spell_and_grants_storm_on_the_third_spell() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([
        {"id":"source","card":"CP01-029"},
        {"id":"before","card":"BP01-125"},
        {"id":"third","card":"CP01-034"}
    ]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"searched","card":"CP01-034"}, {"id":"other","card":"BP01-037"}
    ]);
    initial["players"]["P1"]["zones"]["evolve_deck"] =
        json!([{"id":"carrot-0","card":"CSD01-032"}]);
    initial["players"]["P2"]["zones"]["field"] =
        json!([{"id":"enemy","card":"BP01-037","state":{"hp":20_u8}}]);
    let mut game = Game::from_selected_effects(
        selection().clone(),
        &initial,
        &Value::Null,
        &Value::Null,
        "layer3-daiwa",
    )
    .unwrap();
    let first = question(
        game.play_internal_card("before", PlayerId { seat: 1 })
            .unwrap(),
    );
    let InputDomain::Targets(targets) = &first.domain else {
        panic!("enemy target");
    };
    let follower = targets
        .members
        .iter()
        .find(|member| member.card.is_some())
        .unwrap()
        .handle;
    resolved(
        game.answer_internal_card(first.actor, first.token, Answer::Targets(vec![follower]))
            .unwrap(),
    );
    finish_checks(&mut game);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    let search = question(pending(&mut game, "CP01-029"));
    let InputDomain::Search(domain) = &search.domain else {
        panic!("Uma spell search");
    };
    assert_eq!(domain.cards.len(), 1);
    assert_eq!(domain.cards[0].card, "CP01-034");
    resolved(
        game.answer_internal_card(
            search.actor,
            search.token,
            Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle])),
        )
        .unwrap(),
    );
    finish_checks(&mut game);
    let second = game
        .play_internal_card("searched", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, second));
    finish_checks(&mut game);
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        4_u8
    );
    assert!(game.internal_pending_choice().unwrap().is_none());
    let third = game
        .play_internal_card("third", PlayerId { seat: 1 })
        .unwrap();
    resolved(finish(&mut game, third));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        3_u8
    );
    let trigger = pending(&mut game, "CP01-029");
    resolved(finish(&mut game, trigger));
    finish_checks(&mut game);
    assert!(
        game.projection(View::Referee).unwrap()["objects"]["source"]["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("疾走"))
    );
    let meal = meal_race::feed(&mut game, "CP01-029", 0, false);
    assert!(
        meal.progress
            .facts
            .iter()
            .any(|fact| matches!(fact, Fact::Raced(_)))
    );
    meal_race::settle(&mut game);
    let after = game.projection(View::Referee).unwrap();
    assert_eq!(after["P1"]["pp"]["current"], 2_u8);
    assert_eq!(after["P1"]["race"], json!(["carrot-0"]));
}
