#![expect(
    clippy::panic,
    reason = "Unexpected decisions invalidate these printed payment scenarios."
)]
use super::hand_exchange::{exchange_game, finish, position};
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn fanfare(game: &mut Game, number: &str) -> CardPlay {
    let pending = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &pending.domain else {
        panic!("printed Fanfare")
    };
    let handle = options
        .iter()
        .find(|option| option.card == number)
        .unwrap()
        .handle;
    game.answer_internal_card(pending.actor, pending.token, Answer::Pending(handle))
        .unwrap()
}
#[test]
fn bp18_090_uses_original_two_cost_and_keeps_wrong_effective_two_for_all_printings() {
    for number in ["BP18-090", "BP18-P39"] {
        let initial = position(
            number,
            json!([
                {"id":"fuel","card":"exchange-two","state":{"cost":9_u8}},
                {"id":"wrong","card":"exchange-dummy","state":{"cost":2_u8}}
            ]),
            1,
        );
        let mut game = exchange_game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(
            matches!(finish(&mut game, play, true, 1), CardPlay::Entered(_)),
            "one printed-two material discounts entry"
        );
        let trigger = fanfare(&mut game, number);
        resolved(finish(&mut game, trigger, true, 0));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 0_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["fuel"]));
        assert_eq!(view["P1"]["hand"], json!(["wrong", "draw0"]));
    }
}
#[test]
fn cp02_050_discards_before_entry_and_searches_distinct_spell_names_for_all_printings() {
    for number in ["CP02-050", "CP02-P41"] {
        let mut initial = position(number, json!([{"id":"fuel","card":"exchange-dummy"}]), 4);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"spell0","card":"exchange-spell0"},
            {"id":"duplicate","card":"exchange-spell0"},
            {"id":"spell1","card":"exchange-spell1"},
            {"id":"wrong-deck","card":"exchange-dummy"}
        ]);
        let mut game = exchange_game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(
            matches!(finish(&mut game, play, true, 4), CardPlay::Entered(_)),
            "one discard reduces six to four PP"
        );
        let CardPlay::Awaiting(question) = fanfare(&mut game, number) else {
            panic!("private search")
        };
        let InputDomain::Search(domain) = &question.domain else {
            panic!("spell search")
        };
        assert_eq!(domain.cards.len(), 3, "only spells are candidates");
        assert_eq!(
            domain.groups.as_ref().unwrap().len(),
            1,
            "one spell filter owns the grouped answer"
        );
        assert_eq!(domain.cardinality.maximum, Some(Nat(2)), "up to two names");
        let duplicates: Vec<_> = domain
            .cards
            .iter()
            .filter(|card| card.card == "exchange-spell0")
            .map(|card| card.handle)
            .collect();
        let before = game.projection(View::Referee).unwrap();
        game.answer_internal_card(
            question.actor,
            question.token,
            Answer::Search(SearchAnswer::Grouped(vec![duplicates])),
        )
        .unwrap_err();
        assert_eq!(
            game.projection(View::Referee).unwrap(),
            before,
            "repeated names are rejected atomically"
        );
        let picks = vec![
            domain
                .cards
                .iter()
                .find(|card| card.card == "exchange-spell0")
                .unwrap()
                .handle,
            domain
                .cards
                .iter()
                .find(|card| card.card == "exchange-spell1")
                .unwrap()
                .handle,
        ];
        resolved(
            game.answer_internal_card(
                question.actor,
                question.token,
                Answer::Search(SearchAnswer::Grouped(vec![picks])),
            )
            .unwrap(),
        );
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 0_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["fuel"]));
        assert_eq!(
            view["P1"]["hand"].as_array().unwrap().len(),
            2,
            "two distinct spells enter hand"
        );
        assert!(
            view["P1"]["hand"]
                .as_array()
                .unwrap()
                .contains(&json!("spell1")),
            "second name is found"
        );
    }
}
#[test]
fn cp02_sp06a_discards_three_then_resolves_damage_and_draw_for_all_printings() {
    for number in ["CP02-SP06a", "CP02-SP06b", "CP02-U06a", "CP02-U06b"] {
        let initial = position(
            number,
            json!([
                {"id":"fuel0","card":"exchange-idol"},{"id":"fuel1","card":"exchange-idol"},{"id":"fuel2","card":"exchange-idol"}
            ]),
            3,
        );
        let mut game = exchange_game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        assert!(
            matches!(finish(&mut game, play, true, 3), CardPlay::Entered(_)),
            "three discards reduce nine to three PP"
        );
        let trigger = fanfare(&mut game, number);
        resolved(finish(&mut game, trigger, true, 0));
        let view = game.projection(View::Referee).unwrap();
        assert_eq!(view["P1"]["pp"]["current"], 0_u8);
        assert_eq!(view["P1"]["cemetery"], json!(["fuel0", "fuel1", "fuel2"]));
        assert_eq!(view["objects"]["enemy"]["damage"], 4_u8);
        assert_eq!(view["P1"]["hand"], json!(["draw0", "draw1"]));
    }
}
#[test]
fn bp20_018_optional_hand_extra_changes_damage_and_always_draws_for_all_printings() {
    for number in ["BP20-018", "BP20-P12"] {
        for accepted in [false, true] {
            let initial = position(
                number,
                json!([{"id":"fuel","card":"exchange-beast"},{"id":"wrong","card":"exchange-dummy"}]),
                2,
            );
            let mut game = exchange_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            resolved(finish(&mut game, play, accepted, 2));
            let view = game.projection(View::Referee).unwrap();
            assert_eq!(
                view["P1"]["pp"]["current"], 0_u8,
                "optional extra does not discount PP"
            );
            assert_eq!(
                view["objects"]["enemy"]["damage"],
                if accepted { 5_u8 } else { 3_u8 }
            );
            assert_eq!(
                view["P1"]["deck"].as_array().unwrap().len(),
                1,
                "both paths draw one"
            );
            assert_eq!(
                view["P1"]["cemetery"],
                if accepted {
                    json!(["fuel", "source"])
                } else {
                    json!(["source"])
                }
            );
        }
    }
}
