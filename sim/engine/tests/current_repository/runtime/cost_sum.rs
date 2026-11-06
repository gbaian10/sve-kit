#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

const PLAYER: PlayerId = PlayerId { seat: 1 };

fn trigger(game: &mut Game, card: &str) -> DecisionView {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("expected the printed fanfare of {card}");
    };
    let option = options
        .iter()
        .find(|option| option.card == card)
        .unwrap_or_else(|| panic!("expected a trigger from {card}"));
    let CardPlay::Awaiting(choice) = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
    else {
        panic!("the cost-capped Look choice must be offered");
    };
    choice
}

fn handle(question: &DecisionView, card: &str) -> u64 {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a card choice");
    };
    domain
        .members
        .iter()
        .find(|member| member.card.as_deref() == Some(card))
        .unwrap_or_else(|| panic!("{card} must be a candidate"))
        .handle
}

fn bounds(question: &DecisionView) -> (Nat, Option<Nat>) {
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a card choice");
    };
    (domain.cardinality.minimum, domain.cardinality.maximum)
}

/// Rejected answers must leave the same question open.
fn rejects(game: &mut Game, question: &DecisionView, handles: Vec<u64>) {
    let before = format!("{game:?}");
    game.answer_internal_card(question.actor, question.token, Answer::Targets(handles))
        .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "a rejected answer leaves the choice open"
    );
}

fn look_play(card: &str, deck: Value) -> (Game, DecisionView) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":card}]);
    initial["players"]["P1"]["zones"]["deck"] = deck;
    let mut game = game(&initial);
    assert!(
        matches!(
            game.play_internal_card("source", PLAYER).unwrap(),
            CardPlay::Entered(_)
        ),
        "{card} enters before its fanfare"
    );
    let question = trigger(&mut game, card);
    (game, question)
}

fn order_rest(game: &mut Game, play: CardPlay) {
    let CardPlay::Awaiting(order) = play else {
        panic!("the unchosen Look cards must be ordered");
    };
    let InputDomain::Order(cards) = &order.domain else {
        panic!("expected the private bottom order");
    };
    let handles = cards.iter().map(|card| card.handle).collect();
    resolved(
        game.answer_internal_card(order.actor, order.token, Answer::Order(handles))
            .unwrap(),
    );
}

#[test]
fn csd02b_002_puts_cool_followers_whose_original_costs_total_at_most_four() {
    let (mut game, question) = look_play(
        "CSD02b-002",
        json!([
            {"id":"two","card":"CP02-042"},
            {"id":"three","card":"CP02-039"},
            {"id":"one","card":"CSD02b-007"},
            {"id":"four","card":"CP02-099"},
            {"id":"other","card":"BP01-037"},
            {"id":"tail","card":"BP01-037"}
        ]),
    );
    assert_eq!(bounds(&question), (Nat(0), Some(Nat(2))));
    let (two, three, one, four) = (
        handle(&question, "CP02-042"),
        handle(&question, "CP02-039"),
        handle(&question, "CSD02b-007"),
        handle(&question, "CP02-099"),
    );
    // A non-Cool card is never offered, and 2+3 or 1+4 exceed the printed total of 4.
    let InputDomain::Targets(domain) = &question.domain else {
        panic!("expected a card choice");
    };
    assert_eq!(domain.members.len(), 4);
    rejects(&mut game, &question, vec![two, three]);
    rejects(&mut game, &question, vec![one, four]);
    let play = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(vec![three, one]))
        .unwrap();
    order_rest(&mut game, play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "three", "one"]));
    assert_eq!(state["P1"]["deck"].as_array().unwrap().len(), 4);
    assert_eq!(state["P1"]["deck"][0], json!("tail"));
}

#[test]
fn bp21_025_rejects_a_pair_over_the_total_but_accepts_one_card_at_the_cap() {
    let (mut game, question) = look_play(
        "BP21-025",
        json!([
            {"id":"one","card":"BP08-042"},
            {"id":"three","card":"BP21-009"},
            {"id":"four","card":"DSD01a-010"},
            {"id":"other","card":"CP02-039"},
            {"id":"filler","card":"BP01-037"}
        ]),
    );
    assert_eq!(bounds(&question), (Nat(0), Some(Nat(2))));
    let (one, four) = (
        handle(&question, "BP08-042"),
        handle(&question, "DSD01a-010"),
    );
    rejects(&mut game, &question, vec![four, one]);
    let play = game
        .answer_internal_card(question.actor, question.token, Answer::Targets(vec![four]))
        .unwrap();
    order_rest(&mut game, play);
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["source", "four"]));
    assert!(
        state["P1"]["deck"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == "three")
    );
}

fn cemetery_play(cemetery: Value) -> (Game, DecisionView) {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP22-091"}]);
    initial["players"]["P1"]["zones"]["cemetery"] = cemetery;
    let mut game = game(&initial);
    let CardPlay::Awaiting(question) = game.play_internal_card("source", PLAYER).unwrap() else {
        panic!("the spell must declare its cemetery targets");
    };
    (game, question)
}

#[test]
fn bp22_091_declares_departed_targets_within_the_total_and_gives_them_rush() {
    let (mut game, question) = cemetery_play(json!([
        {"id":"one","card":"BP01-118"},
        {"id":"two","card":"BP07-081"},
        {"id":"three","card":"BP04-093"},
        {"id":"five","card":"BP01-107"},
        {"id":"other","card":"BP01-037"}
    ]));
    assert_eq!(bounds(&question), (Nat(0), Some(Nat(3))));
    let (one, two, three, five) = (
        handle(&question, "BP01-118"),
        handle(&question, "BP07-081"),
        handle(&question, "BP04-093"),
        handle(&question, "BP01-107"),
    );
    rejects(&mut game, &question, vec![two, five]);
    rejects(&mut game, &question, vec![one, two, five]);
    resolved(
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Targets(vec![one, two, three]),
        )
        .unwrap(),
    );
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["field"], json!(["one", "two", "three"]));
    assert_eq!(state["P1"]["cemetery"], json!(["five", "other", "source"]));
    for id in ["one", "two", "three"] {
        assert!(
            state["objects"][id]["keywords"]
                .as_array()
                .unwrap()
                .contains(&json!("突進")),
            "{id} must have Rush"
        );
    }
}
