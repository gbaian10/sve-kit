#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed Look union scenarios."
)]
use super::*;
use sve_engine::game::internal::DecisionView;

fn game(state: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        state,
        &Value::Null,
        &Value::Null,
        "look-union",
    )
    .unwrap()
}

fn awaiting(play: CardPlay) -> DecisionView {
    let CardPlay::Awaiting(q) = play else {
        panic!("printed decision")
    };
    q
}

fn position(source_zone: &str, source: &str, deck: &[(&str, &str)]) -> Value {
    let mut state = draw_position();
    state["players"]["P1"]["zones"][source_zone] = json!([{"id":"source","card":source}]);
    state["players"]["P1"]["zones"]["deck"] = json!(
        deck.iter()
            .map(|(id, card)| json!({"id":id,"card":card}))
            .collect::<Vec<_>>()
    );
    state
}

fn fanfare(game: &mut Game, number: &str) -> DecisionView {
    let mut play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    if let CardPlay::Awaiting(q) = &play
        && matches!(q.domain, InputDomain::WardEntry { .. })
    {
        play = game
            .answer_internal_card(q.actor, q.token, Answer::WardEntry(false))
            .unwrap();
    }
    assert!(matches!(play, CardPlay::Entered(_)), "{number} enters first");
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("fanfare")
    };
    let handle = options.iter().find(|o| o.card == number).unwrap().handle;
    awaiting(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
            .unwrap(),
    )
}

/// Picks every candidate and returns their printed numbers in domain order.
fn pick_all(game: &mut Game, q: &DecisionView) -> (Vec<String>, CardPlay) {
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("observed choice")
    };
    assert!(
        game.internal_decision(View::P2).is_none(),
        "the opponent gets no private choice"
    );
    let cards = domain
        .members
        .iter()
        .map(|m| m.card.clone().unwrap())
        .collect();
    let handles = domain.members.iter().map(|m| m.handle).collect();
    let next = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(handles))
        .unwrap();
    (cards, next)
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only ordering and entry questions belong to these remainders."
)]
fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::Order(cards) => Answer::Order(cards.iter().map(|c| c.handle).collect()),
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            _ => panic!("unexpected question"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
}

#[test]
fn bp02_008_offers_either_printed_trait_from_one_look() {
    let state = position(
        "hand",
        "BP02-008",
        &[("fairy", "BP01-013"), ("crystal", "BP02-001"), ("tail", "BP01-171")],
    );
    let mut game = game(&state);
    let q = fanfare(&mut game, "BP02-008");
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("optional union")
    };
    assert_eq!(domain.cardinality.maximum, Some(Nat(1)));
    let crystal = domain
        .members
        .iter()
        .find(|m| m.card.as_deref() == Some("BP02-001"))
        .unwrap()
        .handle;
    assert_eq!(domain.members.len(), 2, "both trait branches qualify");
    let next = game
        .answer_internal_card(q.actor, q.token, Answer::Targets(vec![crystal]))
        .unwrap();
    finish(&mut game, next);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["crystal"]));
    assert_eq!(view["P1"]["deck"], json!(["tail", "fairy"]));
}

#[test]
fn bp21_010_puts_both_cost_one_trait_branches_onto_the_field() {
    let mut state = position(
        "field",
        "BP21-009",
        &[
            ("academy", "BP21-033"),
            ("beast", "BP09-010"),
            ("goblin", "BP01-171"),
            ("spell", "BP02-011"),
            ("cheap-fairy", "BP01-013"),
            ("tail", "BP01-171"),
        ],
    );
    state["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP21-010"}]);
    let mut game = game(&state);
    game.internal_rule_action(
        &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
    )
    .unwrap();
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("evolve trigger")
    };
    let handle = options
        .iter()
        .find(|o| o.card == "BP21-010")
        .unwrap()
        .handle;
    let choice = awaiting(
        game.answer_internal_card(q.actor, q.token, Answer::Pending(handle))
            .unwrap(),
    );
    let (cards, next) = pick_all(&mut game, &choice);
    assert_eq!(cards, ["BP21-033", "BP09-010"], "goblin and fairy fail both branches");
    finish(&mut game, next);
    let view = game.projection(View::Referee).unwrap();
    let field = view["P1"]["field"].as_array().unwrap();
    assert!(field.contains(&json!("academy")) && field.contains(&json!("beast")));
    assert_eq!(view["P1"]["deck"].as_array().unwrap().len(), 4);
}

#[test]
fn bp05_003_takes_at_most_one_elf_follower_and_one_elf_spell() {
    let state = position(
        "hand",
        "BP05-003",
        &[
            ("follower", "BP01-013"),
            ("spell", "BP01-007"),
            ("amulet", "BP01-016"),
            ("goblin", "BP01-171"),
            ("tail", "BP01-171"),
        ],
    );
    let mut game = game(&state);
    let q = fanfare(&mut game, "BP05-003");
    let InputDomain::Targets(domain) = &q.domain else {
        panic!("grouped choice")
    };
    assert_eq!(domain.cardinality.maximum, Some(Nat(2)));
    let (cards, next) = pick_all(&mut game, &q);
    assert_eq!(cards, ["BP01-013", "BP01-007"], "the Elf amulet fits neither group");
    finish(&mut game, next);
    let view = game.projection(View::Referee).unwrap();
    assert_eq!(view["P1"]["hand"], json!(["follower", "spell"]));
    assert_eq!(view["P1"]["deck"][0], json!("tail"));
}
