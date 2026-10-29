#![expect(
    clippy::panic,
    reason = "Unexpected decision domains invalidate these printed-card scenarios."
)]

use super::*;
use sve_engine::game::internal::DecisionView;

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
fn bp22_031_places_distinct_cost_limited_followers_and_orders_the_remainder() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-031"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"cost_two","card":"BP02-118"},
        {"id":"cost_three","card":"BP01-009"},
        {"id":"too_expensive","card":"BP01-037"},
        {"id":"other_remainder","card":"BP01-034"},
        {"id":"tail","card":"BP01-037"}
    ]);
    let mut game = game(&initial);
    let CardPlay::Awaiting(ward) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("printed Ward must be handled before the fanfare");
    };
    assert!(matches!(ward.domain, InputDomain::WardEntry { .. }));
    assert!(matches!(
        game.answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
            .unwrap(),
        CardPlay::Entered(_)
    ));

    let CardPlay::Awaiting(question) = trigger(&mut game, "BP22-031") else {
        panic!("the grouped cost-limited Look choice must be offered");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected the private grouped Look choice");
    };
    assert_eq!(domain.cardinality.minimum, Nat(0));
    assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
    assert_eq!(
        domain
            .members
            .iter()
            .map(|member| member.card.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["BP02-118", "BP01-034", "BP01-009"]
    );
    let cost_two = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP02-118"))
        .unwrap()
        .handle;
    let cost_three = domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some("BP01-009"))
        .unwrap()
        .handle;

    let CardPlay::Awaiting(order) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![cost_two, cost_three]),
        )
        .unwrap()
    else {
        panic!("the unselected Look cards must be ordered");
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
    assert!(referee["P1"]["field"].as_array().unwrap().iter().any(|object| object == "cost_two"));
    assert!(referee["P1"]["field"].as_array().unwrap().iter().any(|object| object == "cost_three"));
    assert_eq!(referee["P1"]["deck"], json!(["tail", "other_remainder", "too_expensive"]));
}

fn grouped_question(deck: Value) -> (Game, DecisionView) {
    grouped_question_with_field(deck, json!([]))
}

fn grouped_question_with_field(deck: Value, field: Value) -> (Game, DecisionView) {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":7_u8,"max":7_u8});
    initial["players"]["P1"]["zones"]["field"] = field;
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-031"}]);
    initial["players"]["P1"]["zones"]["deck"] = deck;
    let mut game = game(&initial);
    let CardPlay::Awaiting(ward) = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap()
    else {
        panic!("printed Ward must be handled before the fanfare");
    };
    game.answer_internal_card(ward.actor, ward.token, Answer::WardEntry(false))
        .unwrap();
    let CardPlay::Awaiting(question) = trigger(&mut game, "BP22-031") else {
        panic!("the grouped cost-limited Look choice must be offered");
    };
    (game, question)
}

fn handles_of(question: &DecisionView, card: &str) -> Vec<u64> {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected the private grouped Look choice");
    };
    domain
        .members
        .iter()
        .filter(|member| member.card.as_deref() == Some(card))
        .map(|member| member.handle)
        .collect()
}

fn two_threes_deck() -> Value {
    json!([
        {"id":"three_a","card":"BP01-009"},
        {"id":"three_b","card":"BP01-009"},
        {"id":"too_expensive","card":"BP01-037"},
        {"id":"one","card":"BP01-034"},
        {"id":"tail","card":"BP01-037"}
    ])
}

#[test]
fn bp22_031_rejects_two_followers_that_only_fit_the_cost_three_group() {
    let (mut game, question) = grouped_question(two_threes_deck());
    let threes = handles_of(&question, "BP01-009");
    assert_eq!(threes.len(), 2);
    assert_eq!(handles_of(&question, "BP01-037"), Vec::<u64>::new());
    game.answer_internal_card(question.actor, question.token, Answer::Targets(threes))
        .unwrap_err();
}

#[test]
fn bp22_031_accepts_a_cost_three_follower_with_a_cost_one_follower() {
    let (mut game, question) = grouped_question(two_threes_deck());
    let one = handles_of(&question, "BP01-034")[0];
    let three = handles_of(&question, "BP01-009")[0];
    assert!(matches!(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![three, one]),
        )
        .unwrap(),
        CardPlay::Awaiting(_)
    ));
}

#[test]
fn bp22_031_declining_places_nothing_and_orders_all_four_cards() {
    let (mut game, question) = grouped_question(two_threes_deck());
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
        .unwrap()
    else {
        panic!("all looked-at cards must be ordered");
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("expected a private bottom-order question");
    };
    assert_eq!(cards.len(), 4);
    let referee = game.projection(View::Referee).unwrap();
    assert!(
        !referee["P1"]["field"]
            .as_array()
            .unwrap()
            .iter()
            .any(|object| object == "three_a")
    );
}

#[test]
fn bp22_031_caps_selection_when_the_field_has_no_free_slots() {
    let field = json!([
        {"id":"existing_one","card":"BP01-037"},
        {"id":"existing_two","card":"BP01-037"},
        {"id":"existing_three","card":"BP01-037"},
        {"id":"existing_four","card":"BP01-037"}
    ]);
    let (mut game, question) = grouped_question_with_field(two_threes_deck(), field);
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected the private grouped Look choice");
    };
    assert_eq!(domain.cardinality.maximum, Some(Nat(0)));
    let CardPlay::Awaiting(order) = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(vec![]))
        .unwrap()
    else {
        panic!("all looked-at cards must be ordered when the field is full");
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("expected a private bottom-order question");
    };
    assert_eq!(cards.len(), 4);
}

#[test]
fn bp22_032_evolved_printing_deals_five_to_an_opposing_follower() {
    let mut initial = draw_position();
    initial["players"]["P1"]["pp"] = json!({"current":1_u8,"max":1_u8});
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP22-031"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP22-032"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
    let mut game = game(&initial);
    let step = game
        .internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
        )
        .unwrap();
    assert_eq!(step.outcome, "resolved");
    let CardPlay::Awaiting(question) = trigger(&mut game, "BP22-032") else {
        panic!("BP22-032 must ask for an opposing follower");
    };
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected the printed follower target");
    };
    assert_eq!(domain.members.len(), 1);
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
        5_u8
    );
}
