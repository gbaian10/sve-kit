#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::{DecisionView, SearchAnswer};

fn search_question(game: &mut Game, card: &str) -> DecisionView {
    while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
        game.resume_internal_card().unwrap();
    }
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("expected the printed search trigger");
    };
    let option = options
        .iter()
        .find(|option| option.card == card)
        .unwrap_or_else(|| panic!("expected a trigger from {card}"));
    let CardPlay::Awaiting(question) = game
        .answer_internal_card(
            pending.actor,
            pending.token,
            Answer::Pending(option.handle),
        )
        .unwrap()
    else {
        panic!("the cost-capped search must be asked");
    };
    question
}

fn handle(question: &DecisionView, card: &str) -> u64 {
    let InputDomain::Search(domain) = &question.domain else {
        panic!("expected a search, got {:?}", question.domain);
    };
    domain
        .cards
        .iter()
        .find(|candidate| candidate.card == card)
        .unwrap_or_else(|| panic!("{card} must be found"))
        .handle
}

fn found_cards(question: &DecisionView) -> Vec<String> {
    let InputDomain::Search(domain) = &question.domain else {
        panic!("expected a search, got {:?}", question.domain);
    };
    let mut cards = domain
        .cards
        .iter()
        .map(|card| card.card.clone())
        .collect::<Vec<_>>();
    cards.sort_unstable();
    cards
}

fn answer(found: Vec<u64>) -> Answer {
    // One group spans the whole filtered deck so the total applies to every found card.
    Answer::Search(SearchAnswer::Grouped(vec![found]))
}

/// Rejected answers must leave the same question open.
fn rejects(game: &mut Game, question: &DecisionView, found: Vec<u64>) {
    let before = format!("{game:?}");
    game.answer_internal_card(question.actor, question.token, answer(found))
        .unwrap_err();
    assert_eq!(
        format!("{game:?}"),
        before,
        "a rejected search leaves the question open"
    );
}

fn on_field(game: &Game, ids: &[&str]) -> bool {
    let state = game.projection(View::Referee).unwrap();
    let field = state["P1"]["field"].as_array().unwrap();
    ids.iter().all(|id| field.contains(&json!(id)))
}

#[test]
fn bp18_025_evolution_searches_tokyo_followers_whose_original_costs_total_at_most_three() {
    for form in ["BP18-025", "BP18-P10"] {
        let mut initial = draw_position();
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP18-024"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"one","card":"BP18-071"},
            {"id":"two","card":"BP18-062"},
            {"id":"other","card":"BP01-037"},
            {"id":"two_b","card":"BP18-060"}
        ]);
        let mut game = game(&initial);
        assert_eq!(
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}})
            )
            .unwrap()
            .outcome,
            "resolved",
            "{form} evolves from the admitted base"
        );
        let question = search_question(&mut game, form);
        assert_eq!(
            found_cards(&question),
            ["BP18-060", "BP18-062", "BP18-071"],
            "only Tokyo followers are found"
        );
        let (one, two, two_b) = (
            handle(&question, "BP18-071"),
            handle(&question, "BP18-062"),
            handle(&question, "BP18-060"),
        );
        rejects(&mut game, &question, vec![two, two_b]);
        resolved(
            game.answer_internal_card(question.actor, question.token, answer(vec![one, two]))
                .unwrap(),
        );
        assert!(
            on_field(&game, &["one", "two"]),
            "both found followers enter the field"
        );
    }
}

#[test]
fn ecp01_051_race_searches_mejiro_followers_or_amulets_within_a_total_of_six() {
    for card in ["ECP01-051", "ECP01-P27"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":card}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] =
            json!([{"id":"carrot","card":"CSD01-032"}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"amulet","card":"ECP01-054"},
            {"id":"follower","card":"CP01-072"},
            {"id":"five","card":"ECP01-048"},
            {"id":"other","card":"BP01-037"}
        ]);
        let mut game = Game::from_selected_effects(
            selection_with_rules().clone(),
            &initial,
            &Value::Null,
            &Value::Null,
            "search-cost-sum",
        )
        .unwrap();
        meal_race::feed(&mut game, card, 0, false);
        let question = search_question(&mut game, card);
        assert_eq!(
            found_cards(&question),
            ["CP01-072", "ECP01-048", "ECP01-054"],
            "one search finds Mejiro followers and amulets"
        );
        let (amulet, follower, five) = (
            handle(&question, "ECP01-054"),
            handle(&question, "CP01-072"),
            handle(&question, "ECP01-048"),
        );
        rejects(&mut game, &question, vec![five, amulet]);
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                answer(vec![amulet, follower]),
            )
            .unwrap(),
        );
        assert!(
            on_field(&game, &["amulet", "follower"]),
            "a Mejiro amulet and follower enter together"
        );
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(
            (
                state["objects"]["source"]["power"].as_i64().unwrap(),
                state["objects"]["source"]["hp"].as_i64().unwrap()
            ),
            (4, 4),
            "the race also gives the source +1/+1"
        );
    }
}
