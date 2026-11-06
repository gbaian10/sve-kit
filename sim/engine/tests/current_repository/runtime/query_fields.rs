#![expect(
    clippy::panic,
    clippy::wildcard_enum_match_arm,
    reason = "Unexpected decisions invalidate the printed scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

const PLAYER: PlayerId = PlayerId { seat: 1 };
/// ゲオルギウス (BP11-055) evolves for 2 PP.
const EVOLVE_PP: u8 = 2;

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "query-fields",
    )
    .unwrap()
}

fn evolve(game: &mut Game, source: &str, form: &str, pp: u8) {
    assert_eq!(
        game.internal_rule_action(
            &json!({"do":"evolve","source":source,"evolve_card":form,"pay":{"pp":pp}})
        )
        .unwrap()
        .outcome,
        "resolved",
        "{source} evolves"
    );
}

/// Resolves pending triggers and answers single-card targets with the first candidate.
fn settle(game: &mut Game, mut step: Option<CardPlay>) {
    loop {
        while let Some(CardPlay::Awaiting(question)) = &step {
            let answer = match &question.domain {
                InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
                InputDomain::WardEntry { .. } => Answer::WardEntry(false),
                // BP18-025's own evolution search finds nothing in these empty decks.
                InputDomain::Search(domain) => Answer::Search(domain.groups.as_ref().map_or_else(
                    || SearchAnswer::Ungrouped(vec![]),
                    |groups| SearchAnswer::Grouped(vec![vec![]; groups.len()]),
                )),
                other => panic!("unexpected question {other:?}"),
            };
            step = Some(
                game.answer_internal_card(question.actor, question.token, answer)
                    .unwrap(),
            );
        }
        while game.internal_card_stage() == Some(ResumeStage::CheckTiming) {
            game.resume_internal_card().unwrap();
        }
        let Some(pending) = game.internal_pending_choice().unwrap() else {
            return;
        };
        let InputDomain::Pending(options) = &pending.domain else {
            panic!("expected a trigger");
        };
        step = Some(
            game.answer_internal_card(pending.actor, pending.token, Answer::Pending(options[0].handle))
                .unwrap(),
        );
    }
}

// Negative cases for `evolved` and `traits_all` live in the synthetic
// internal_ir `filter_guards` and `query_conditions` tests.
#[test]
fn bp13_080_fanfare_buffs_while_an_evolved_follower_is_on_the_field() {
    let mut initial = draw_position();
    initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP13-080"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"base","card":"BP18-024"}]);
    initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":"BP18-025"}]);
    let mut game = rules_game(&initial);
    evolve(&mut game, "base", "form", 1);
    settle(&mut game, None);
    let step = game.play_internal_card("source", PLAYER).unwrap();
    settle(&mut game, Some(step));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["objects"]["source"]["power"], 3_u8);
    assert_eq!(state["objects"]["source"]["hp"], 3_u8);
}

#[test]
fn bp12_011_discounts_with_an_ex_fairy_princess_and_damages_by_the_fairy_attack() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP12-011"}]);
    initial["players"]["P1"]["zones"]["field"] = json!([{"id":"fairy","card":"BP01-T03"}]);
    // BP16-001 has both traits.
    initial["players"]["P1"]["zones"]["ex"] = json!([{"id":"princess","card":"BP16-001"}]);
    initial["players"]["P2"]["zones"]["field"] = json!([
        {"id":"enemy","card":"CP03-013","state":{"hp":20_u8,"max_hp":20_u8}}
    ]);
    let mut game = rules_game(&initial);
    let step = game.play_internal_card("source", PLAYER).unwrap();
    settle(&mut game, Some(step));
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["pp"]["current"], 7_u8, "4 - 1 PP");
    assert_eq!(state["objects"]["enemy"]["damage"], 1_u8, "the fairy token is 1/1");
}

#[test]
fn bp11_056_evolution_destroys_only_enemies_with_three_or_less_life() {
    for form in ["BP11-056", "BP11-P14"] {
        let mut initial = draw_position();
        initial["turn"]["elapsed_turns"] = json!({"P1":7_u8,"P2":6_u8});
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP11-055"}]);
        initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"one","card":"BP17-077"},
            {"id":"three","card":"BP21-009"},
            {"id":"five","card":"BP01-037"}
        ]);
        let mut game = rules_game(&initial);
        evolve(&mut game, "source", "form", EVOLVE_PP);
        settle(&mut game, None);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P2"]["field"], json!(["five"]), "{form}");
    }
}

#[test]
fn ecp02_072_destroys_every_follower_without_the_deremas_trait() {
    for printing in ["ECP02-072", "ECP02-SP13", "ECP02-SL33", "ECP02-U13"] {
        let mut initial = draw_position();
        initial["players"]["P1"]["leader"]["life"] = json!(10_u8);
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":printing}]);
        initial["players"]["P1"]["zones"]["deck"] = json!([
            {"id":"draw","card":"BP01-037"},
            {"id":"draw_b","card":"BP01-037"}
        ]);
        initial["players"]["P1"]["zones"]["cemetery"] = json!(
            (0_u8..10)
                .map(|i| json!({"id":format!("idol{i}"),"card":"CP02-068"}))
                .collect::<Vec<_>>()
        );
        initial["players"]["P1"]["zones"]["field"] = json!([
            {"id":"idol","card":"CP02-039"},
            {"id":"other","card":"BP01-037"}
        ]);
        initial["players"]["P2"]["zones"]["field"] = json!([
            {"id":"enemy","card":"BP01-037"},
            {"id":"enemy_idol","card":"CP02-014"}
        ]);
        let mut game = rules_game(&initial);
        let step = game.play_internal_card("source", PLAYER).unwrap();
        settle(&mut game, Some(step));
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["field"], json!(["idol"]), "{printing}");
        assert_eq!(state["P2"]["field"], json!(["enemy_idol"]));
        // Ten デレマス cards in the cemetery: 12 - 5 = 7 PP.
        assert_eq!(state["P1"]["pp"]["current"], 3_u8);
        assert_eq!(state["P1"]["ex"].as_array().unwrap().len(), 2);
        assert_eq!(state["P1"]["leader"]["life"], 14_u8);
        assert!(state["P1"]["hand"].as_array().unwrap().contains(&json!("draw_b")));
    }
}
