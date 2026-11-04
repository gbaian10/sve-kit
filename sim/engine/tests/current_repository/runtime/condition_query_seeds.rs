#![expect(
    clippy::panic,
    reason = "Unexpected printed questions invalidate these card scenarios."
)]
use super::*;
use sve_engine::game::internal::SearchAnswer;

fn finish(game: &mut Game, mut play: CardPlay) {
    while let CardPlay::Awaiting(q) = play {
        let answer = match &q.domain {
            InputDomain::WardEntry { .. } => Answer::WardEntry(false),
            InputDomain::Targets(domain) => Answer::Targets(vec![domain.members[0].handle]),
            InputDomain::Search(domain) => {
                assert_eq!(
                    domain.cards.len(),
                    1,
                    "the dragon spell search has exactly one eligible card"
                );
                Answer::Search(SearchAnswer::Ungrouped(vec![domain.cards[0].handle]))
            }
            InputDomain::Abilities(_)
            | InputDomain::ResourceEntryPayment { .. }
            | InputDomain::Quick { .. }
            | InputDomain::OptionalCost
            | InputDomain::CostMaterials(_)
            | InputDomain::BeforeDraw
            | InputDomain::Pending(_)
            | InputDomain::DamageReplacements(_)
            | InputDomain::TriggerQuota(_)
            | InputDomain::Integer(_)
            | InputDomain::Order(_)
            | InputDomain::DriveTrigger { .. }
            | InputDomain::SearchPartition(_)
            | InputDomain::Distribution(_)
            | InputDomain::SearchReplacement
            | InputDomain::TokenCapacity { .. }
            | InputDomain::Modes { .. } => panic!("unexpected question"),
        };
        play = game.answer_internal_card(q.actor, q.token, answer).unwrap();
    }
    assert!(
        !matches!(play, CardPlay::Cancelled),
        "the printed play has legal targets and payment"
    );
}

fn rules_game(initial: &Value) -> Game {
    Game::from_selected_effects(
        selection_with_rules().clone(),
        initial,
        &Value::Null,
        &Value::Null,
        "condition-title",
    )
    .unwrap()
}

fn pending(game: &mut Game, number: &str) {
    let q = game.internal_pending_choice().unwrap().unwrap();
    let InputDomain::Pending(options) = &q.domain else {
        panic!("pending")
    };
    let option = options.iter().find(|option| option.card == number).unwrap();
    let play = game
        .answer_internal_card(q.actor, q.token, Answer::Pending(option.handle))
        .unwrap();
    finish(game, play);
}

#[test]
fn bp02_112_checks_the_selected_followers_current_evolved_kind() {
    for number in ["BP02-112", "PR-047"] {
        for evolved in [false, true] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP07-031","state":{"evolved":evolved,"evolved_with":if evolved {json!("form")}else{Value::Null}}}]);
            if evolved {
                initial["players"]["P2"]["zones"]["evolution"] =
                    json!([{"id":"form","card":"BP07-032"}]);
            }
            let mut game = game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                if evolved { 4_u8 } else { 3_u8 }
            );
        }
    }
}

#[test]
fn bp09_060_checks_the_selected_followers_trait_after_real_evolution() {
    for form in ["BP09-060", "BP09-P19", "DSD01b-015", "SD08-015"] {
        for (enemy, damage) in [("BP01-086", 5_u8), ("BP01-037", 2)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"source","card":"BP09-059"}]);
            initial["players"]["P1"]["zones"]["evolve_deck"] = json!([{"id":"form","card":form}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":enemy}]);
            let mut game = game(&initial);
            game.internal_rule_action(
                &json!({"do":"evolve","source":"source","evolve_card":"form","pay":{"pp":1_u8}}),
            )
            .unwrap();
            pending(&mut game, form);
            assert_eq!(
                game.projection(View::Referee).unwrap()["objects"]["enemy"]["damage"],
                damage
            );
        }
    }
}

#[test]
fn cp02_011_draws_then_recovers_pp_only_with_own_original_cost_five_follower() {
    for number in ["CP02-011", "CP02-P08"] {
        for (follower, pp) in [("BP01-037", 10_u8), ("BP01-038", 9)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own","card":follower}]);
            initial["players"]["P2"]["zones"]["field"] = json!([{"id":"enemy","card":"BP01-037"}]);
            initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
            let mut game = game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["hand"], json!(["drawn"]));
            assert_eq!(state["P1"]["pp"]["current"], pp);
        }
    }
}

#[test]
fn cp02_031_discounts_with_cost_one_and_stands_own_followers() {
    for number in ["CP02-031", "CP02-P25", "CSD02c-012"] {
        for (follower, remaining) in [("BP01-038", 4_u8), ("BP01-037", 1)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] =
                json!([{"id":"own","card":follower,"state":{"acted":true}}]);
            initial["players"]["P2"]["zones"]["field"] =
                json!([{"id":"enemy","card":"BP01-038","state":{"acted":true}}]);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            pending(&mut game, number);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(state["P1"]["pp"]["current"], remaining);
            assert_eq!(state["objects"]["own"]["acted"], false);
            assert_eq!(state["objects"]["enemy"]["acted"], true);
        }
    }
}

#[test]
fn cp03_027_name_part_condition_grants_power_and_storm_only_with_a_blaster() {
    for number in ["CP03-027", "CP03-P17"] {
        for (other, matches) in [("CP03-086", true), ("BP01-037", false)] {
            let mut initial = draw_position();
            initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":number}]);
            initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own","card":other}]);
            let mut game = rules_game(&initial);
            let play = game
                .play_internal_card("source", PlayerId { seat: 1 })
                .unwrap();
            finish(&mut game, play);
            pending(&mut game, number);
            let state = game.projection(View::Referee).unwrap();
            assert_eq!(
                state["objects"]["source"]["power"],
                if matches { 4_u8 } else { 3 }
            );
            assert_eq!(
                state["objects"]["source"]["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("疾走")),
                matches
            );
        }
    }
}

#[test]
fn bp17_108_name_part_discount_and_faith_count_damage_use_own_field() {
    for (other, remaining) in [("BP17-096", 10_u8), ("BP01-143", 9)] {
        let mut initial = draw_position();
        initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP17-108"}]);
        initial["players"]["P1"]["zones"]["field"] = json!([{"id":"own","card":other}]);
        initial["players"]["P2"]["zones"]["field"] =
            json!([{"id":"enemy","card":"BP04-117"},{"id":"foreign-marlone","card":"BP17-096"}]);
        let mut game = game(&initial);
        let play = game
            .play_internal_card("source", PlayerId { seat: 1 })
            .unwrap();
        finish(&mut game, play);
        let state = game.projection(View::Referee).unwrap();
        assert_eq!(state["P1"]["pp"]["current"], remaining);
        assert_eq!(state["objects"]["enemy"]["damage"], 1_u8);
    }
}

#[test]
fn bp09_059_companion_searches_only_the_printed_cost_two_dragon_spell() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP09-059"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"found","card":"BP09-058"},{"id":"wrong-class","card":"BP01-071"},{"id":"wrong-cost","card":"BP01-090"},{"id":"wrong-kind","card":"BP01-086"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    pending(&mut game, "BP09-059");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["found"]));
    let deck = state["P1"]["deck"].as_array().unwrap();
    assert_eq!(deck.len(), 3);
    for id in ["wrong-class", "wrong-cost", "wrong-kind"] {
        assert!(deck.contains(&json!(id)));
    }
    assert_eq!(state["P1"]["pp"]["current"], 7_u8);
}

#[test]
fn bp17_096_companion_creates_the_existing_advisor_and_resolves_its_real_fanfare() {
    let mut initial = draw_position();
    initial["players"]["P1"]["zones"]["hand"] = json!([{"id":"source","card":"BP17-096"}]);
    initial["players"]["P1"]["zones"]["deck"] = json!([{"id":"drawn","card":"BP01-037"}]);
    let mut game = game(&initial);
    let play = game
        .play_internal_card("source", PlayerId { seat: 1 })
        .unwrap();
    finish(&mut game, play);
    pending(&mut game, "BP17-096");
    game.resume_internal_card().unwrap();
    pending(&mut game, "BP17-T09");
    let state = game.projection(View::Referee).unwrap();
    assert_eq!(state["P1"]["hand"], json!(["drawn"]));
    assert_eq!(state["P1"]["field"].as_array().unwrap().len(), 2);
    assert_eq!(state["P1"]["pp"]["current"], 6_u8);
}
