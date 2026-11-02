#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Look remainder scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn game(state: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "look-remainder",
    )
    .unwrap()
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed decision")
    };
    q
}

fn fanfare(game: &mut Game, number: &str) -> CardPlay {
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    assert!(matches!(play, CardPlay::Entered(_)), "{number} enters first");
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("fanfare")
    };
    let handle = options.iter().find(|o| o.card == number).unwrap().handle;
    game.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
        .unwrap()
}

fn looked(number: &str) -> (Game, DecisionView) {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    state["players"]["P1"]["zones"]["deck"] = json!([
        {"id":"one","card":"BP01-171"}, {"id":"two","card":"BP01-034"},
        {"id":"three","card":"BP01-037"}, {"id":"tail","card":"BP01-171"}
    ]);
    let mut game = game(&state);
    let q = awaiting(fanfare(&mut game, number));
    (game, q)
}

fn targets(game: &mut Game, q: &DecisionView, picked: &[&str]) -> CardPlay {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("observed choice")
    };
    let handles = domain
        .members
        .iter()
        .filter(|m| picked.contains(&m.card.as_deref().unwrap()))
        .map(|m| m.handle)
        .collect();
    game.answer_internal_card(q.actor, q.token, Answer::Targets(handles))
        .unwrap()
}

fn order(game: &mut Game, play: CardPlay, top_to_bottom: &[&str]) -> CardPlay {
    let q = awaiting(play);
    let InputDomain::Order(cards) = &q.domain else {
        panic!("private order")
    };
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent gets no private order"
    );
    let handles = top_to_bottom
        .iter()
        .map(|card| cards.iter().find(|c| c.card == *card).unwrap().handle)
        .collect();
    game.answer_internal_card(q.actor, q.token, Answer::Order(handles))
        .unwrap()
}

#[test]
fn cp01_070_orders_chosen_cards_on_top_and_the_rest_on_the_bottom() {
    let (mut game, q) = looked("CP01-070");
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("any number")
    };
    assert_eq!(domain.cardinality.minimum, Nat(0));
    assert_eq!(domain.cardinality.maximum, Some(Nat(3)));
    let top = targets(&mut game, &q, &["BP01-171", "BP01-037"]);
    let bottom = order(&mut game, top, &["BP01-037", "BP01-171"]);
    resolved(order(&mut game, bottom, &["BP01-034"]));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["deck"],
        json!(["three", "one", "tail", "two"])
    );
}

#[test]
fn cp02_081_keeps_one_card_on_top_and_mills_the_rest() {
    let (mut game, q) = looked("CP02-081");
    let top = targets(&mut game, &q, &["BP01-034"]);
    resolved(order(&mut game, top, &["BP01-034"]));
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["deck"], json!(["two", "tail"]));
    assert_eq!(view["P1"]["cemetery"], json!(["one", "three"]));
    let opponent = game.projection(View::P2).unwrap();
    let known = opponent["known_cards"].as_array().unwrap();
    assert!(
        !known.iter().any(|card| card["id"] == "two"),
        "the kept card stays private"
    );
}

fn end_turn(game: &mut Game) {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-phase"}))
        .unwrap();
    if let Some(question) = game.internal_quick_choice().unwrap() {
        game.answer_internal_card(question.actor, question.token, Answer::Quick(None))
            .unwrap();
    }
    game.internal_rule_action(&json!({"do":"end-discard","select":[]}))
        .unwrap();
}

#[test]
fn bp01_084_discount_on_the_placed_card_has_no_turn_limit() {
    let mut state = draw_position();
    state["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP01-084"}]);
    for seat in ["P1", "P2"] {
        state["players"][seat]["zones"]["deck"] = json!(
            (0_u8..3)
                .map(|i| json!({"id":format!("{seat}-draw{i}"),"card":"BP01-171"}))
                .collect::<Vec<_>>()
        );
    }
    state["players"]["P1"]["zones"]["deck"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"id":"placed","card":"BP01-037"}));
    let mut game = game(&state);
    resolved(fanfare(&mut game, "BP01-084"));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["ex"],
        json!(["placed"])
    );
    end_turn(&mut game);
    end_turn(&mut game);
    let before = game.projection(View::Referee).unwrap()["P1"]["pp"]["current"]
        .as_i64()
        .unwrap();
    assert!(matches!(
        game.play_internal_card("placed", PlayerId { seat: 1 })
            .unwrap(),
        CardPlay::Entered(_)
    ));
    assert_eq!(
        game.projection(View::Referee).unwrap()["P1"]["pp"]["current"],
        before - 3,
        "printed cost 5 minus 2 on a later turn"
    );
}
