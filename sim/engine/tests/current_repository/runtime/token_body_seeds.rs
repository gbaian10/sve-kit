#![expect(
    clippy::panic,
    reason = "Unexpected questions invalidate these real-card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const fn actor() -> PlayerId {
    PlayerId { seat: 1 }
}

fn body_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "token-body-seeds",
    )
    .unwrap()
}

fn position(number: &str) -> Value {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    initial
}

fn finish(game: &mut Game, mut play: CardPlay) -> CardPlay {
    while let CardPlay::Awaiting(question) = play {
        let answer = if matches!(question.domain, InputDomain::WardEntry { .. }) {
            Answer::WardEntry(false)
        } else if matches!(question.domain, InputDomain::OptionalCost) {
            Answer::OptionalCost(true)
        } else if let InputDomain::Targets(targets) = &question.domain {
            Answer::Targets(
                targets
                    .members
                    .iter()
                    .take(1)
                    .map(|member| member.handle)
                    .collect(),
            )
        } else if let InputDomain::Search(search) = &question.domain {
            Answer::Search(SearchAnswer::Ungrouped(
                search
                    .cards
                    .iter()
                    .take(1)
                    .map(|card| card.handle)
                    .collect(),
            ))
        } else {
            panic!("unexpected printed choice");
        };
        play = game
            .answer_internal_card(question.actor, question.token, answer)
            .unwrap();
    }
    play
}

fn enter(game: &mut Game) {
    let play = game.play_internal_card("source", actor()).unwrap();
    assert!(
        matches!(finish(game, play), CardPlay::Entered(_)),
        "the real follower completes its paid play"
    );
}

fn trigger(game: &mut Game) {
    let question = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &question.domain else {
        panic!("printed trigger");
    };
    assert_eq!(options.len(), 1, "only the printed trigger is pending");
    let play = game
        .answer_internal_card(
            question.actor,
            question.token,
            Answer::Pending(options.first().unwrap().handle),
        )
        .unwrap();
    assert!(
        matches!(finish(game, play), CardPlay::Resolved(_)),
        "the printed trigger resolves completely"
    );
}

#[test]
fn master_trainer_printings_create_five_real_items_and_recover_four_pp() {
    for number in ["CP02-105", "CP02-P85"] {
        let mut game = body_game(&position(number));
        enter(&mut game);
        trigger(&mut game);
        let state = game.projection(View::Referee).unwrap();
        let items = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(items.len(), 5);
        assert!(
            items
                .iter()
                .all(|id| state["objects"][id.as_str().unwrap()]["card"] == "CP02-T01")
        );
        assert_eq!(state["P1"]["pp"]["current"], 6_u8);
        assert_eq!(state["P2"]["ex"], json!([]));
    }
}

#[test]
fn veteran_trainer_creates_two_items_and_the_created_canonical_item_heals_and_draws() {
    for number in ["CP02-106", "CP02-P86"] {
        let mut initial = position(number);
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        let mut game = body_game(&initial);
        enter(&mut game);
        trigger(&mut game);
        game.resume_internal_card().unwrap();
        let before = game.projection(View::Referee).unwrap();
        let items = before["P1"]["ex"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        let item = items.first().unwrap().as_str().unwrap();
        assert_eq!(before["objects"][item]["card"], "CP02-T01");
        resolved(game.play_internal_card(item, actor()).unwrap());
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P1"]["leader"]["life"], 11_u8);
        assert_eq!(after["P1"]["hand"], json!(["drawn"]));
        assert_eq!(after["P1"]["ex"].as_array().unwrap().len(), 1);
        assert_eq!(after["P1"]["pp"]["current"], 2_u8);
        assert_eq!(after["P1"]["cemetery"], json!([]));
    }
}

#[test]
fn alyaska_printings_evolve_then_create_a_complete_terminal_weapon_even_after_a_search_miss() {
    for number in ["BP10-022", "BP10-SL08"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP10-021"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":number}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"unrelated","card":"BP01-037"}]);
        initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
        let mut game = body_game(&initial);
        game.internal_rule_action(
            &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":2_u8}}),
        )
        .unwrap();
        trigger(&mut game);
        game.resume_internal_card().unwrap();
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["deck"], json!(["unrelated"]));
        let tokens = state["P1"]["ex"].as_array().unwrap();
        assert_eq!(tokens.len(), 1);
        let weapon = tokens.first().unwrap().as_str().unwrap();
        assert_eq!(state["objects"][weapon]["card"], "BP10-T01");
        game.play_internal_card(weapon, PlayerId { seat: 2 })
            .unwrap_err();
        assert_eq!(game.projection(View::Referee).unwrap(), state);
        let entry = game.play_internal_card(weapon, actor()).unwrap();
        assert!(matches!(finish(&mut game, entry), CardPlay::Entered(_)));
        trigger(&mut game);
        let after = game.projection(View::Referee).unwrap();
        assert_eq!(after["P2"]["field"], json!([]));
        assert_eq!(after["P2"]["cemetery"], json!(["enemy"]));
        assert_eq!(after["objects"][weapon]["power"], 6_u8);
        assert_eq!(after["objects"][weapon]["hp"], 6_u8);
    }
}
