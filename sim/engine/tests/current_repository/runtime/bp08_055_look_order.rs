#![expect(
    clippy::panic,
    reason = "Unexpected decision domains invalidate these printed-card scenarios."
)]

use super::*;

fn awakened_position(base: &str, form: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":base}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
    initial
}

fn trigger(game: &mut Game, card: &str) -> CardPlay {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the card's printed trigger");
    };
    let option = options
        .iter()
        .find(|option| option.card == card)
        .unwrap_or_else(|| panic!("expected trigger from {card}"));
    game.answer_internal_card(
        question.actor,
        question.token,
        Answer::Pending(option.handle),
    )
    .unwrap()
}

#[test]
fn bp08_055_filters_its_private_top_three_and_orders_only_the_remainder() {
    let mut initial = awakened_position("BP08-055", "BP08-056");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"dragonnewt","card":"BP08-056"},
        {"id":"third","card":"BP01-037"},
        {"id":"tail","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    let CardPlay::Awaiting(question) = trigger(&mut game, "BP08-055") else {
        panic!("the observed trait choice must ask the controller");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected the filtered Look choice");
    };
    assert_eq!(domain.cardinality.minimum, Nat(0));
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("BP08-056"));
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap()
    else {
        panic!("the remaining observed cards must be ordered");
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("expected a private bottom-order question");
    };
    assert_eq!(cards.len(), 2);
    let order_handles = cards.iter().rev().map(|card| card.handle).collect();
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(order_handles),
        )
        .unwrap(),
    );
    let referee = game.projection(View::Referee).unwrap();
    assert_eq!(referee["P1"]["hand"], json!(["dragonnewt"]));
    assert_eq!(referee["P1"]["deck"], json!(["tail", "third", "first"]));
    let opponent = game.projection(View::P2).unwrap();
    assert!(opponent["known_cards"].as_array().unwrap().iter().any(|card| {
        card["id"] == "dragonnewt" && card["card"] == "BP08-056"
    }));
}

#[test]
fn bp08_055_can_decline_the_private_trait_choice_and_order_all_three_cards() {
    let mut initial = awakened_position("SP01-030", "SP01-031");
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"first","card":"BP01-037"},
        {"id":"dragonnewt","card":"BP08-056"},
        {"id":"third","card":"BP01-037"},
        {"id":"tail","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    assert!(matches!(
        game.play_internal_card("source", PlayerId { seat: 1 }).unwrap(),
        CardPlay::Entered(_)
    ));
    let CardPlay::Awaiting(question) = trigger(&mut game, "SP01-030") else {
        panic!("the alias printing uses the representative trigger");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected the filtered Look choice");
    };
    assert_eq!(domain.members.len(), 1);
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
        .unwrap()
    else {
        panic!("declining still orders all observed cards");
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("expected private ordering");
    };
    assert_eq!(cards.len(), 3);
    let order_handles = cards.iter().rev().map(|card| card.handle).collect();
    resolved(
        game.answer_internal_card(
            order.actor,
            order.token,
            Answer::Order(order_handles),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!([]));
    assert_eq!(state["P1"]["deck"], json!(["tail", "third", "dragonnewt", "first"]));
}

#[test]
fn bp08_056_evolved_printing_targets_an_opposing_follower_for_two_damage() {
    let mut initial = awakened_position("BP08-055", "SP01-031");
    initial["players"]["P1"]["zones"]["hand"] = json!([]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP08-055"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(step.outcome, "resolved");
    let CardPlay::Awaiting(question) = trigger(&mut game, "SP01-031") else {
        panic!("the evolved alias must ask for an opposing follower");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected the printed follower target");
    };
    assert_eq!(domain.members.len(), 1);
    assert_eq!(domain.members[0].card.as_deref(), Some("BP01-037"));
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![domain.members[0].handle]),
        )
        .unwrap(),
    );
    assert_eq!(
        game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
        2_u8
    );
}
